import { Action } from "./action"
import { Application } from "./application"
import { Binding } from "./binding"
import { Controller } from "./controller"
import { ErrorHandler } from "./error_handler"
import { Module } from "./module"
import { Schema } from "./schema"
import { Scope } from "./scope"
import { ValueDescriptor } from "./value_properties"
import { Multimap } from "../multimap"
import {
  AttributeObserver,
  AttributeObserverDelegate,
  SelectorObserver,
  SelectorObserverDelegate,
  StringMapObserver,
  StringMapObserverDelegate,
  Token,
  TokenListObserver,
  TokenListObserverDelegate,
  ValueListObserver,
  ValueListObserverDelegate,
} from "../mutation-observers"
import { readInheritableStaticArrayValues } from "./inheritable_statics"
import { capitalize, namespaceCamelize } from "./string_helpers"

type OutletObserverDetails = { outletName: string }

// Context is the single per-controller object: it owns the controller, its
// scope and all of the wiring (actions, values, targets and outlets) that a
// running controller instance needs.

export class Context
  implements
    ErrorHandler,
    ValueListObserverDelegate<Action>,
    StringMapObserverDelegate,
    TokenListObserverDelegate,
    AttributeObserverDelegate,
    SelectorObserverDelegate
{
  readonly module: Module
  readonly scope: Scope
  readonly controller: Controller

  // Action wiring
  private valueListObserver?: ValueListObserver<Action>
  private bindingsByAction: Map<Action, Binding>

  // Value wiring
  private stringMapObserver: StringMapObserver
  private valueDescriptorMap: { [attributeName: string]: ValueDescriptor }

  // Target wiring
  private readonly targetsByName: Multimap<string, Element>
  private tokenListObserver?: TokenListObserver

  // Outlet wiring
  private outletObservationStarted: boolean
  private readonly outletsByName: Multimap<string, Controller>
  private readonly outletElementsByName: Multimap<string, Element>
  private selectorObserverMap: Map<string, SelectorObserver>
  private attributeObserverMap: Map<string, AttributeObserver>

  constructor(module: Module, scope: Scope) {
    this.module = module
    this.scope = scope
    this.controller = new module.controllerConstructor(this)

    this.bindingsByAction = new Map()

    this.stringMapObserver = new StringMapObserver(this.element, this)
    this.valueDescriptorMap = (this.controller as any).valueDescriptorMap

    this.targetsByName = new Multimap()

    this.outletObservationStarted = false
    this.outletsByName = new Multimap()
    this.outletElementsByName = new Multimap()
    this.selectorObserverMap = new Map()
    this.attributeObserverMap = new Map()

    try {
      this.controller.initialize()
      this.logDebugActivity("initialize")
    } catch (error: any) {
      this.handleError(error, "initializing controller")
    }
  }

  // Lifecycle

  connect() {
    if (!this.valueListObserver) {
      this.valueListObserver = new ValueListObserver(this.element, this.actionAttribute, this)
      this.valueListObserver.start()
    }

    this.stringMapObserver.start()
    this.invokeChangedCallbacksForDefaultValues()

    if (!this.tokenListObserver) {
      this.tokenListObserver = new TokenListObserver(this.element, this.attributeName, this)
      this.tokenListObserver.start()
    }

    if (!this.outletObservationStarted) {
      this.outletDefinitions.forEach((outletName) => {
        this.setupSelectorObserverForOutlet(outletName)
        this.setupAttributeObserverForOutlet(outletName)
      })
      this.outletObservationStarted = true
      this.dependentContexts.forEach((context) => context.refresh())
    }

    try {
      this.controller.connect()
      this.logDebugActivity("connect")
    } catch (error: any) {
      this.handleError(error, "connecting controller")
    }
  }

  refresh() {
    this.selectorObserverMap.forEach((observer) => observer.refresh())
    this.attributeObserverMap.forEach((observer) => observer.refresh())
  }

  disconnect() {
    try {
      this.controller.disconnect()
      this.logDebugActivity("disconnect")
    } catch (error: any) {
      this.handleError(error, "disconnecting controller")
    }

    if (this.outletObservationStarted) {
      this.outletObservationStarted = false
      this.disconnectAllOutlets()
      this.stopSelectorObservers()
      this.stopAttributeObservers()
    }

    if (this.tokenListObserver) {
      this.disconnectAllTargets()
      this.tokenListObserver.stop()
      delete this.tokenListObserver
    }

    this.stringMapObserver.stop()

    if (this.valueListObserver) {
      this.valueListObserver.stop()
      delete this.valueListObserver
      this.disconnectAllActions()
    }
  }

  get application(): Application {
    return this.module.application
  }

  get identifier(): string {
    return this.module.identifier
  }

  get schema(): Schema {
    return this.application.schema
  }

  get element(): Element {
    return this.scope.element
  }

  get parentElement(): Element | null {
    return this.element.parentElement
  }

  // Error handling

  handleError(error: Error, message: string, detail: object = {}) {
    const { identifier, controller, element } = this
    detail = Object.assign({ identifier, controller, element }, detail)
    this.application.handleError(error, `Error ${message}`, detail)
  }

  // Debug logging

  logDebugActivity = (functionName: string, detail: object = {}): void => {
    const { identifier, controller, element } = this
    detail = Object.assign({ identifier, controller, element }, detail)
    this.application.logDebugActivity(this.identifier, functionName, detail)
  }

  // Action management

  get actionAttribute(): string {
    return this.schema.actionAttribute
  }

  get bindings(): Binding[] {
    return Array.from(this.bindingsByAction.values())
  }

  private connectAction(action: Action) {
    const binding = new Binding(this, action)
    this.bindingsByAction.set(action, binding)
    this.application.bindingConnected(binding)
  }

  private disconnectAction(action: Action) {
    const binding = this.bindingsByAction.get(action)
    if (binding) {
      this.bindingsByAction.delete(action)
      // Clear the dispatcher's cached event listener when the action's element
      // has left the document. Keeping the cache for connected elements
      // preserves { once: true } firing history across action attribute
      // updates, but caching listeners for removed elements would retain the
      // element (and everything it references) for the application's lifetime.
      this.application.bindingDisconnected(binding, !action.element.isConnected)
    }
  }

  private disconnectAllActions() {
    this.bindings.forEach((binding) => this.application.bindingDisconnected(binding, true))
    this.bindingsByAction.clear()
  }

  // Value management

  private get valueDescriptors() {
    const { valueDescriptorMap } = this
    return Object.keys(valueDescriptorMap).map((key) => valueDescriptorMap[key])
  }

  private get valueDescriptorNameMap() {
    const descriptors: { [type: string]: ValueDescriptor } = {}

    Object.keys(this.valueDescriptorMap).forEach((key) => {
      const descriptor = this.valueDescriptorMap[key]
      descriptors[descriptor.name] = descriptor
    })

    return descriptors
  }

  private hasValue(attributeName: string) {
    const descriptor = this.valueDescriptorNameMap[attributeName]
    const hasMethodName = `has${capitalize(descriptor.name)}`

    return (this.controller as any)[hasMethodName]
  }

  private invokeChangedCallbacksForDefaultValues() {
    for (const { key, name, defaultValue, writer } of this.valueDescriptors) {
      if (defaultValue != undefined && !this.controller.data.has(key)) {
        this.invokeChangedCallback(name, writer(defaultValue), undefined)
      }
    }
  }

  private invokeChangedCallback(name: string, rawValue: string, rawOldValue: string | undefined) {
    const changedMethodName = `${name}Changed`
    const changedMethod = (this.controller as any)[changedMethodName]

    if (typeof changedMethod == "function") {
      const descriptor = this.valueDescriptorNameMap[name]

      try {
        const value = descriptor.reader(rawValue)
        let oldValue = rawOldValue

        if (rawOldValue) {
          oldValue = descriptor.reader(rawOldValue)
        }

        changedMethod.call(this.controller, value, oldValue)
      } catch (error) {
        if (error instanceof TypeError) {
          error.message = `Stimulus Value "${this.identifier}.${descriptor.name}" - ${error.message}`
        }

        throw error
      }
    }
  }

  // Target management

  private get attributeName() {
    return `data-${this.identifier}-target`
  }

  private connectTarget(element: Element, name: string) {
    if (!this.targetsByName.has(name, element)) {
      this.targetsByName.add(name, element)
      this.tokenListObserver?.pause(() => this.targetConnected(element, name))
    }
  }

  private disconnectTarget(element: Element, name: string) {
    if (this.targetsByName.has(name, element)) {
      this.targetsByName.delete(name, element)
      this.tokenListObserver?.pause(() => this.targetDisconnected(element, name))
    }
  }

  private disconnectAllTargets() {
    for (const name of this.targetsByName.keys) {
      for (const element of this.targetsByName.getValuesForKey(name)) {
        this.disconnectTarget(element, name)
      }
    }
  }

  // Target observer delegate

  targetConnected(element: Element, name: string) {
    this.invokeControllerMethod(`${name}TargetConnected`, element)
  }

  targetDisconnected(element: Element, name: string) {
    this.invokeControllerMethod(`${name}TargetDisconnected`, element)
  }

  // Outlet observer delegate

  outletConnected(outlet: Controller, element: Element, name: string) {
    this.invokeControllerMethod(`${namespaceCamelize(name)}OutletConnected`, outlet, element)
  }

  outletDisconnected(outlet: Controller, element: Element, name: string) {
    this.invokeControllerMethod(`${namespaceCamelize(name)}OutletDisconnected`, outlet, element)
  }

  // Outlet management

  private connectOutlet(outlet: Controller, element: Element, outletName: string) {
    if (!this.outletElementsByName.has(outletName, element)) {
      this.outletsByName.add(outletName, outlet)
      this.outletElementsByName.add(outletName, element)
      this.selectorObserverMap
        .get(outletName)
        ?.pause(() => this.outletConnected(outlet, element, outletName))
    }
  }

  private disconnectOutlet(outlet: Controller, element: Element, outletName: string) {
    if (this.outletElementsByName.has(outletName, element)) {
      this.outletsByName.delete(outletName, outlet)
      this.outletElementsByName.delete(outletName, element)
      this.selectorObserverMap
        .get(outletName)
        ?.pause(() => this.outletDisconnected(outlet, element, outletName))
    }
  }

  private disconnectAllOutlets() {
    for (const outletName of this.outletElementsByName.keys) {
      for (const element of this.outletElementsByName.getValuesForKey(outletName)) {
        for (const outlet of this.outletsByName.getValuesForKey(outletName)) {
          this.disconnectOutlet(outlet, element, outletName)
        }
      }
    }
  }

  private stopSelectorObservers() {
    if (this.selectorObserverMap.size > 0) {
      this.selectorObserverMap.forEach((observer) => observer.stop())
      this.selectorObserverMap.clear()
    }
  }

  private stopAttributeObservers() {
    if (this.attributeObserverMap.size > 0) {
      this.attributeObserverMap.forEach((observer) => observer.stop())
      this.attributeObserverMap.clear()
    }
  }

  private updateSelectorObserverForOutlet(outletName: string) {
    const observer = this.selectorObserverMap.get(outletName)

    if (observer) {
      observer.selector = this.outletSelector(outletName)
    }
  }

  private setupSelectorObserverForOutlet(outletName: string) {
    const selector = this.outletSelector(outletName)
    const selectorObserver = new SelectorObserver(document.body, selector!, this, { outletName })

    this.selectorObserverMap.set(outletName, selectorObserver)

    selectorObserver.start()
  }

  private setupAttributeObserverForOutlet(outletName: string) {
    const attributeName = this.attributeNameForOutletName(outletName)
    const attributeObserver = new AttributeObserver(this.scope.element, attributeName, this)

    this.attributeObserverMap.set(outletName, attributeObserver)

    attributeObserver.start()
  }

  private outletSelector(outletName: string) {
    return this.scope.outlets.getSelectorForOutletName(outletName)
  }

  private attributeNameForOutletName(outletName: string) {
    return this.schema.outletAttributeForScope(this.identifier, outletName)
  }

  private getOutletNameFromOutletAttributeName(attributeName: string) {
    return this.outletDefinitions.find((outletName) => this.attributeNameForOutletName(outletName) === attributeName)
  }

  private get outletDependencies() {
    const dependencies = new Multimap<string, string>()

    this.application.modules.forEach((module) => {
      const constructor = module.definition.controllerConstructor
      const outlets = readInheritableStaticArrayValues(constructor, "outlets")

      outlets.forEach((outlet) => dependencies.add(outlet, module.identifier))
    })

    return dependencies
  }

  private get outletDefinitions() {
    return this.outletDependencies.getKeysForValue(this.identifier)
  }

  private get dependentControllerIdentifiers() {
    return this.outletDependencies.getValuesForKey(this.identifier)
  }

  private get dependentContexts() {
    const identifiers = this.dependentControllerIdentifiers
    return this.application.contexts.filter((context) => identifiers.includes(context.identifier))
  }

  private hasOutlet(element: Element, outletName: string) {
    return !!this.getOutlet(element, outletName) || !!this.getOutletFromMap(element, outletName)
  }

  private getOutlet(element: Element, outletName: string) {
    return this.application.getControllerForElementAndIdentifier(element, outletName)
  }

  private getOutletFromMap(element: Element, outletName: string) {
    return this.outletsByName.getValuesForKey(outletName).find((outlet) => outlet.element === element)
  }

  // Action observer delegate

  parseValueForToken(token: Token): Action | undefined {
    const action = Action.forToken(token, this.schema)
    if (action.identifier == this.identifier) {
      return action
    }
  }

  elementMatchedValue(element: Element, action: Action) {
    this.connectAction(action)
  }

  elementUnmatchedValue(element: Element, action: Action) {
    this.disconnectAction(action)
  }

  // String map observer delegate

  getStringMapKeyForAttribute(attributeName: string) {
    if (attributeName in this.valueDescriptorMap) {
      return this.valueDescriptorMap[attributeName].name
    }
  }

  stringMapKeyAdded(key: string, attributeName: string) {
    const descriptor = this.valueDescriptorMap[attributeName]

    if (!this.hasValue(key)) {
      this.invokeChangedCallback(key, descriptor.writer((this.controller as any)[key]), descriptor.writer(descriptor.defaultValue))
    }
  }

  stringMapValueChanged(value: string, name: string, oldValue: string) {
    const descriptor = this.valueDescriptorNameMap[name]

    if (value === null) return

    if (oldValue === null) {
      oldValue = descriptor.writer(descriptor.defaultValue)
    }

    this.invokeChangedCallback(name, value, oldValue)
  }

  stringMapKeyRemoved(key: string, attributeName: string, oldValue: string) {
    const descriptor = this.valueDescriptorNameMap[key]

    if (this.hasValue(key)) {
      this.invokeChangedCallback(key, descriptor.writer((this.controller as any)[key]), oldValue)
    } else {
      this.invokeChangedCallback(key, descriptor.writer(descriptor.defaultValue), oldValue)
    }
  }

  // Target observer delegate

  tokenMatched({ element, content: name }: Token) {
    if (this.scope.containsElement(element)) {
      this.connectTarget(element, name)
    }
  }

  tokenUnmatched({ element, content: name }: Token) {
    this.disconnectTarget(element, name)
  }

  // Outlet attribute observer delegate

  elementMatchedAttribute(_element: Element, attributeName: string) {
    const outletName = this.getOutletNameFromOutletAttributeName(attributeName)

    if (outletName) {
      this.updateSelectorObserverForOutlet(outletName)
    }
  }

  elementAttributeValueChanged(_element: Element, attributeName: string) {
    const outletName = this.getOutletNameFromOutletAttributeName(attributeName)

    if (outletName) {
      this.updateSelectorObserverForOutlet(outletName)
    }
  }

  elementUnmatchedAttribute(_element: Element, attributeName: string) {
    const outletName = this.getOutletNameFromOutletAttributeName(attributeName)

    if (outletName) {
      this.updateSelectorObserverForOutlet(outletName)
    }
  }

  // Outlet selector observer delegate

  selectorMatched(element: Element, _selector: string, { outletName }: OutletObserverDetails) {
    const outlet = this.getOutlet(element, outletName)

    if (outlet) {
      this.connectOutlet(outlet, element, outletName)
    }
  }

  selectorUnmatched(element: Element, _selector: string, { outletName }: OutletObserverDetails) {
    const outlet = this.getOutletFromMap(element, outletName)

    if (outlet) {
      this.disconnectOutlet(outlet, element, outletName)
    }
  }

  selectorMatchElement(element: Element, { outletName }: OutletObserverDetails) {
    const selector = this.outletSelector(outletName)
    const hasOutlet = this.hasOutlet(element, outletName)
    const hasOutletController = element.matches(`[${this.schema.controllerAttribute}~=${outletName}]`)

    if (selector) {
      return hasOutlet && hasOutletController && element.matches(selector)
    } else {
      return false
    }
  }

  // Private

  invokeControllerMethod(methodName: string, ...args: any[]) {
    const controller: any = this.controller
    if (typeof controller[methodName] == "function") {
      controller[methodName](...args)
    }
  }
}
