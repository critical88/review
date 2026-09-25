import { Binding } from "./binding"
import { Context } from "./context"
import { Controller, ControllerConstructor } from "./controller"
import { Definition } from "./definition"
import { ErrorHandler } from "./error_handler"
import { EventListener } from "./event_listener"
import { Logger } from "./logger"
import { Module } from "./module"
import { Multimap } from "../multimap"
import { Scope } from "./scope"
import { ScopeObserver, ScopeObserverDelegate } from "./scope_observer"
import { Schema, defaultSchema } from "./schema"
import { ActionDescriptorFilter, ActionDescriptorFilters, defaultActionDescriptorFilters } from "./action_descriptor"

// Application is the root object of a Stimulus installation: it owns the DOM
// scope observation, the module registry and the event dispatching cache, so
// scopes, controllers, bindings and event listeners are all coordinated from
// here.

export class Application implements ErrorHandler, ScopeObserverDelegate {
  readonly element: Element
  readonly schema: Schema
  readonly actionDescriptorFilters: ActionDescriptorFilters
  logger: Logger = console
  debug = false

  private scopeObserver: ScopeObserver
  private scopesByIdentifier: Multimap<string, Scope>
  private modulesByIdentifier: Map<string, Module>
  private eventListenerMaps: Map<EventTarget, Map<string, EventListener>>
  private eventListeningStarted: boolean

  static start(element?: Element, schema?: Schema): Application {
    const application = new this(element, schema)
    application.start()
    return application
  }

  constructor(element: Element = document.documentElement, schema: Schema = defaultSchema) {
    this.element = element
    this.schema = schema
    this.actionDescriptorFilters = { ...defaultActionDescriptorFilters }

    this.eventListenerMaps = new Map()
    this.eventListeningStarted = false

    this.scopesByIdentifier = new Multimap()
    this.modulesByIdentifier = new Map()
    this.scopeObserver = new ScopeObserver(this.element, this.schema, this)
  }

  get dispatcher(): Application {
    // Kept for the call sites that still expect `application.dispatcher` to
    // expose the dispatcher's view of the event listener cache.
    return this
  }

  async start() {
    await domReady()
    this.logDebugActivity("application", "starting")

    if (!this.eventListeningStarted) {
      this.eventListeningStarted = true
      this.eventListeners.forEach((eventListener) => eventListener.connect())
    }

    this.scopeObserver.start()

    this.logDebugActivity("application", "start")
  }

  stop() {
    this.logDebugActivity("application", "stopping")

    if (this.eventListeningStarted) {
      this.eventListeningStarted = false
      this.eventListeners.forEach((eventListener) => eventListener.disconnect())
    }

    this.scopeObserver.stop()

    this.logDebugActivity("application", "stop")
  }

  register(identifier: string, controllerConstructor: ControllerConstructor) {
    this.load({ identifier, controllerConstructor })
  }

  registerActionOption(name: string, filter: ActionDescriptorFilter) {
    this.actionDescriptorFilters[name] = filter
  }

  load(...definitions: Definition[]): void
  load(definitions: Definition[]): void
  load(head: Definition | Definition[], ...rest: Definition[]) {
    const definitions = Array.isArray(head) ? head : [head, ...rest]
    definitions.forEach((definition) => {
      if ((definition.controllerConstructor as any).shouldLoad) {
        this.loadDefinition(definition)
      }
    })
  }

  unload(...identifiers: string[]): void
  unload(identifiers: string[]): void
  unload(head: string | string[], ...rest: string[]) {
    const identifiers = Array.isArray(head) ? head : [head, ...rest]
    identifiers.forEach((identifier) => this.unloadIdentifier(identifier))
  }

  // Controllers

  get controllers(): Controller[] {
    return this.contexts.map((context) => context.controller)
  }

  getControllerForElementAndIdentifier(element: Element, identifier: string): Controller | null {
    const context = this.getContextForElementAndIdentifier(element, identifier)
    return context ? context.controller : null
  }

  // Scopes and modules

  get controllerAttribute(): string {
    return this.schema.controllerAttribute
  }

  get modules() {
    return Array.from(this.modulesByIdentifier.values())
  }

  get contexts() {
    return this.modules.reduce((contexts, module) => contexts.concat(module.contexts), [] as Context[])
  }

  loadDefinition(definition: Definition) {
    this.unloadIdentifier(definition.identifier)
    const module = new Module(this, definition)
    this.connectModule(module)
    const afterLoad = (definition.controllerConstructor as any).afterLoad
    if (afterLoad) {
      afterLoad.call(definition.controllerConstructor, definition.identifier, this)
    }
  }

  unloadIdentifier(identifier: string) {
    const module = this.modulesByIdentifier.get(identifier)
    if (module) {
      this.disconnectModule(module)
    }
  }

  getContextForElementAndIdentifier(element: Element, identifier: string) {
    const module = this.modulesByIdentifier.get(identifier)
    if (module) {
      return module.contexts.find((context) => context.element == element)
    }
  }

  proposeToConnectScopeForElementAndIdentifier(element: Element, identifier: string) {
    const scope = this.scopeObserver.parseValueForElementAndIdentifier(element, identifier)

    if (scope) {
      this.scopeObserver.elementMatchedValue(scope.element, scope)
    } else {
      console.error(`Couldn't find or create scope for identifier: "${identifier}" and element:`, element)
    }
  }

  // Scope observer delegate

  createScopeForElementAndIdentifier(element: Element, identifier: string) {
    return new Scope(this.schema, element, identifier, this.logger)
  }

  scopeConnected(scope: Scope) {
    this.scopesByIdentifier.add(scope.identifier, scope)
    const module = this.modulesByIdentifier.get(scope.identifier)
    if (module) {
      module.connectContextForScope(scope)
    }
  }

  scopeDisconnected(scope: Scope) {
    this.scopesByIdentifier.delete(scope.identifier, scope)
    const module = this.modulesByIdentifier.get(scope.identifier)
    if (module) {
      module.disconnectContextForScope(scope)
    }
  }

  // Modules

  private connectModule(module: Module) {
    this.modulesByIdentifier.set(module.identifier, module)
    const scopes = this.scopesByIdentifier.getValuesForKey(module.identifier)
    scopes.forEach((scope) => module.connectContextForScope(scope))
  }

  private disconnectModule(module: Module) {
    this.modulesByIdentifier.delete(module.identifier)
    const scopes = this.scopesByIdentifier.getValuesForKey(module.identifier)
    scopes.forEach((scope) => module.disconnectContextForScope(scope))
  }

  // Bindings and event listeners

  get eventListeners(): EventListener[] {
    return Array.from(this.eventListenerMaps.values()).reduce(
      (listeners, map) => listeners.concat(Array.from(map.values())),
      [] as EventListener[]
    )
  }

  bindingConnected(binding: Binding) {
    this.fetchEventListenerForBinding(binding).bindingConnected(binding)
  }

  bindingDisconnected(binding: Binding, clearEventListeners = false) {
    this.fetchEventListenerForBinding(binding).bindingDisconnected(binding)
    if (clearEventListeners) this.clearEventListenersForBinding(binding)
  }

  private clearEventListenersForBinding(binding: Binding) {
    const eventListener = this.fetchEventListenerForBinding(binding)
    if (!eventListener.hasBindings()) {
      eventListener.disconnect()
      this.removeMappedEventListenerFor(binding)
    }
  }

  private removeMappedEventListenerFor(binding: Binding) {
    const { eventTarget, eventName, eventOptions } = binding
    const eventListenerMap = this.fetchEventListenerMapForEventTarget(eventTarget)
    const cacheKey = this.cacheKey(eventName, eventOptions)

    eventListenerMap.delete(cacheKey)
    if (eventListenerMap.size == 0) this.eventListenerMaps.delete(eventTarget)
  }

  private fetchEventListenerForBinding(binding: Binding): EventListener {
    const { eventTarget, eventName, eventOptions } = binding
    return this.fetchEventListener(eventTarget, eventName, eventOptions)
  }

  private fetchEventListener(
    eventTarget: EventTarget,
    eventName: string,
    eventOptions: AddEventListenerOptions
  ): EventListener {
    const eventListenerMap = this.fetchEventListenerMapForEventTarget(eventTarget)
    const cacheKey = this.cacheKey(eventName, eventOptions)
    let eventListener = eventListenerMap.get(cacheKey)
    if (!eventListener) {
      eventListener = this.createEventListener(eventTarget, eventName, eventOptions)
      eventListenerMap.set(cacheKey, eventListener)
    }
    return eventListener
  }

  private createEventListener(
    eventTarget: EventTarget,
    eventName: string,
    eventOptions: AddEventListenerOptions
  ): EventListener {
    const eventListener = new EventListener(eventTarget, eventName, eventOptions)
    if (this.eventListeningStarted) {
      eventListener.connect()
    }
    return eventListener
  }

  private fetchEventListenerMapForEventTarget(eventTarget: EventTarget): Map<string, EventListener> {
    let eventListenerMap = this.eventListenerMaps.get(eventTarget)
    if (!eventListenerMap) {
      eventListenerMap = new Map()
      this.eventListenerMaps.set(eventTarget, eventListenerMap)
    }
    return eventListenerMap
  }

  private cacheKey(eventName: string, eventOptions: any): string {
    const parts = [eventName]
    Object.keys(eventOptions)
      .sort()
      .forEach((key) => {
        parts.push(`${eventOptions[key] ? "" : "!"}${key}`)
      })
    return parts.join(":")
  }

  // Error handling

  handleError(error: Error, message: string, detail: object) {
    this.logger.error(`%s\n\n%o\n\n%o`, message, error, detail)

    window.onerror?.(message, "", 0, 0, error)
  }

  // Debug logging

  logDebugActivity = (identifier: string, functionName: string, detail: object = {}): void => {
    if (this.debug) {
      this.logFormattedMessage(identifier, functionName, detail)
    }
  }

  private logFormattedMessage(identifier: string, functionName: string, detail: object = {}) {
    detail = Object.assign({ application: this }, detail)

    this.logger.groupCollapsed(`${identifier} #${functionName}`)
    this.logger.log("details:", { ...detail })
    this.logger.groupEnd()
  }
}

function domReady() {
  return new Promise<void>((resolve) => {
    if (document.readyState == "loading") {
      document.addEventListener("DOMContentLoaded", () => resolve())
    } else {
      resolve()
    }
  })
}
