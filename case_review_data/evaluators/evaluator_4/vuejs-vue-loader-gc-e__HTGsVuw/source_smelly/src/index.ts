import type { LoaderContext } from 'webpack'
import * as path from 'path'
import * as crypto from 'crypto'
import * as qs from 'querystring'
import * as fs from 'fs'

import { compiler } from './compiler'
import type {
  TemplateCompiler,
  CompilerError,
  CompilerOptions,
  SFCBlock,
  SFCDescriptor,
  SFCScriptBlock,
  SFCTemplateCompileOptions,
  SFCScriptCompileOptions,
} from 'vue/compiler-sfc'
import type { ParsedUrlQuery } from 'querystring'
import chalk = require('chalk')

import VueLoaderPlugin from './plugin'
import {
  getOptions,
  stringifyRequest as _stringifyRequest,
  genMatchResource,
  testWebpack5,
  resolveTemplateTSOptions,
} from './util'

export { VueLoaderPlugin }

export interface VueLoaderOptions {
  // https://babeljs.io/docs/en/next/babel-parser#plugins
  babelParserPlugins?: SFCScriptCompileOptions['babelParserPlugins']
  transformAssetUrls?: SFCTemplateCompileOptions['transformAssetUrls']
  compiler?: TemplateCompiler | string
  compilerOptions?: CompilerOptions
  /**
   * TODO remove in 3.4
   * @deprecated
   */
  reactivityTransform?: boolean

  /**
   * @experimental
   */
  propsDestructure?: boolean
  /**
   * @experimental
   */
  defineModel?: boolean

  customElement?: boolean | RegExp

  hotReload?: boolean
  exposeFilename?: boolean
  appendExtension?: boolean
  enableTsInTemplate?: boolean
  experimentalInlineMatchResource?: boolean

  isServerBuild?: boolean
}

const { parse, compileScript, generateCodeFrame } = compiler
const exportHelperPath = require.resolve('./exportHelper')

/**
 * Coordinates the full compilation of a single file component.
 *
 * All state involved in turning an SFC request into a webpack module lives
 * here: the parsed descriptor and its cache, the resolved script blocks, the
 * per-request options, the generated import code and the attached runtime
 * props. Companion loaders (the template loader, the pitcher and the plugin
 * variants for both webpack 4 and 5) also read the shared caches through this
 * class, so they no longer need to import the helper modules individually.
 */
export class SFCLoader {
  // ---- shared state (module singletons owned by the coordinator) ----

  /**
   * Cached descriptors keyed by filename. The descriptor is cached here by
   * the main loader and re-used by the block loaders so that all requests for
   * the same file share one parse result.
   * This function should only be called after the descriptor has been
   * cached by the main loader.
   * If this is somehow called without a cache hit, it's probably due to sub
   * loaders being run in separate threads. The only way to deal with this is
   * to read from disk directly...
   */
  static descriptorCache = new Map<string, SFCDescriptor>()

  /** resolved client script blocks keyed by descriptor identity */
  static clientCache = new WeakMap<SFCDescriptor, SFCScriptBlock | null>()
  /** resolved server script blocks keyed by descriptor identity */
  static serverCache = new WeakMap<SFCDescriptor, SFCScriptBlock | null>()

  /** imported types -> the SFCs depending on them, for HMR */
  static typeDepToSFCMap = new Map<string, Set<string>>()

  /** make sure the missing-plugin error is only emitted once */
  static errorEmitted = false

  // ---- per request state ----

  loaderContext: LoaderContext<VueLoaderOptions>
  source: string
  options: VueLoaderOptions
  sourceMap: boolean | undefined
  rootContext: string | undefined
  resourcePath: string
  isWebpack5: boolean
  incomingQuery: ParsedUrlQuery
  resourceQuery: string
  enableInlineMatchResource: boolean
  isServer: boolean
  isProduction: boolean
  filename: string
  asCustomElement: boolean
  descriptor!: SFCDescriptor
  errors!: (SyntaxError | CompilerError)[]
  rawShortFilePath!: string
  shortFilePath!: string
  id!: string
  hasScoped = false
  needsHotReload = false
  isTS = false
  templateRequest: string | undefined
  propsToAttach: [string, string][] = []

  constructor(loaderContext: LoaderContext<VueLoaderOptions>, source: string) {
    this.loaderContext = loaderContext
    this.source = source
    this.options = (getOptions(loaderContext) || {}) as VueLoaderOptions

    const {
      mode,
      target,
      sourceMap,
      rootContext,
      resourcePath,
      resourceQuery: _resourceQuery = '',
      _compiler,
    } = loaderContext

    this.sourceMap = sourceMap
    this.rootContext = rootContext
    this.resourcePath = resourcePath

    this.isWebpack5 = testWebpack5(_compiler)
    const rawQuery = _resourceQuery.slice(1)
    this.incomingQuery = qs.parse(rawQuery)
    this.resourceQuery = rawQuery ? `&${rawQuery}` : ``
    this.enableInlineMatchResource =
      this.isWebpack5 && Boolean(this.options.experimentalInlineMatchResource)
    this.isServer = this.options.isServerBuild ?? target === 'node'
    this.isProduction =
      mode === 'production' || process.env.NODE_ENV === 'production'
    this.filename = resourcePath.replace(/\?.*$/, '')

    this.asCustomElement =
      typeof this.options.customElement === 'boolean'
        ? this.options.customElement
        : (this.options.customElement || /\.ce\.vue$/).test(this.filename)
  }

  /**
   * Run the whole compilation for the current request and return the code of
   * the generated proxy module, or undefined when the request was answered
   * directly via the loader callback.
   */
  generate(): string | undefined {
    this.ensurePluginInstalled()

    this.loadSFC()
    if (this.errors.length) {
      this.errors.forEach((err) => {
        SFCLoader.formatError(err, this.source, this.loaderContext.resourcePath)
        this.loaderContext.emitError(err)
      })
      return ``
    }

    this.computeScopeId()

    // if the query has a type field, this is a language block request
    // e.g. foo.vue?type=template&id=xxxxx
    // and we will return early
    if (this.incomingQuery.type) {
      this.selectBlock()
      return undefined
    }

    // feature information
    this.hasScoped = this.descriptor.styles.some((s) => s.scoped)
    this.needsHotReload =
      !this.isServer &&
      !this.isProduction &&
      !!(this.descriptor.script || this.descriptor.scriptSetup || this.descriptor.template) &&
      this.options.hotReload !== false

    // extra properties to attach to the script object
    // we need to do this in a tree-shaking friendly manner
    this.propsToAttach = []

    const scriptImport = this.genScriptImport()
    const templateImport = this.genTemplateImport()
    const stylesCode = this.genStyleImports()

    let code = [templateImport, scriptImport, stylesCode]
      .filter(Boolean)
      .join('\n')

    // attach scope Id for runtime use
    if (this.hasScoped) {
      this.propsToAttach.push([`__scopeId`, `"data-v-${this.id}"`])
    }

    // Expose filename. This is used by the devtools and Vue runtime warnings.
    if (!this.isProduction) {
      // Expose the file's full path in development, so that it can be opened
      // from the devtools.
      this.propsToAttach.push([
        `__file`,
        JSON.stringify(this.rawShortFilePath.replace(/\\/g, '/')),
      ])
    } else if (this.options.exposeFilename) {
      // Libraries can opt-in to expose their components' filenames in production builds.
      // For security reasons, only expose the file's basename in production.
      this.propsToAttach.push([
        `__file`,
        JSON.stringify(path.basename(this.resourcePath)),
      ])
    }

    // custom blocks
    if (this.descriptor.customBlocks && this.descriptor.customBlocks.length) {
      code += this.genCustomBlocksCode()
    }

    // finalize
    code = this.finalizeComponentCode(code)

    return code
  }

  private ensurePluginInstalled() {
    if (
      !SFCLoader.errorEmitted &&
      !(this.loaderContext as any)['thread-loader'] &&
      !(this.loaderContext as any)[VueLoaderPlugin.NS]
    ) {
      this.loaderContext.emitError(
        new Error(
          `vue-loader was used without the corresponding plugin. ` +
            `Make sure to include VueLoaderPlugin in your webpack config.`
        )
      )
      SFCLoader.errorEmitted = true
    }
  }

  /** parse the SFC and store the descriptor in the shared cache */
  private loadSFC() {
    const { descriptor, errors } = parse(this.source, {
      filename: this.filename,
      sourceMap: this.sourceMap,
      templateParseOptions: this.options.compilerOptions,
    })
    this.descriptor = descriptor
    this.errors = errors

    this.cacheDescriptor()
  }

  /** cache descriptor so the block loaders can re-use the parse result */
  private cacheDescriptor() {
    SFCLoader.descriptorCache.set(
      SFCLoader.cleanQuery(this.filename),
      this.descriptor
    )
  }

  /**
   * module id for scoped CSS & hot-reload. The id depends on the short file
   * path and, in production mode, on the source content so that builds are
   * stable.
   */
  private computeScopeId() {
    this.rawShortFilePath = path
      .relative(this.rootContext || process.cwd(), this.filename)
      .replace(/^(\.\.[\/\\])+/, '')
    this.shortFilePath = this.rawShortFilePath.replace(/\\/g, '/')
    this.id = SFCLoader.hash(
      this.isProduction
        ? this.shortFilePath + '\n' + this.source.replace(/\r\n/g, '\n')
        : this.shortFilePath
    )
  }

  private stringifyRequest(request: string) {
    return _stringifyRequest(this.loaderContext, request)
  }

  // ---------------------------------------------------------------------------
  // block selection (a query with a type field selects one language block and
  // returns its raw content via the loader callback)
  // ---------------------------------------------------------------------------

  private selectBlock() {
    const query = this.incomingQuery

    // template
    if (query.type === `template`) {
      // if we are receiving a query with type it can only come from a *.vue file
      // that contains that block, so the block is guaranteed to exist.
      const template = this.descriptor.template!
      if (this.options.appendExtension) {
        this.loaderContext.resourcePath += '.' + (template.lang || 'html')
      }
      this.loaderContext.callback(null, template.content, template.map as any)
      return
    }

    // script
    if (query.type === `script`) {
      const script = SFCLoader.resolveScript(
        this.descriptor,
        this.id,
        this.options,
        this.loaderContext
      )!
      if (this.options.appendExtension) {
        this.loaderContext.resourcePath += '.' + (script.lang || 'js')
      }
      this.loaderContext.callback(null, script.content, script.map as any)
      return
    }

    // styles
    if (query.type === `style` && query.index != null) {
      const style = this.descriptor.styles[Number(query.index)]
      if (this.options.appendExtension) {
        this.loaderContext.resourcePath += '.' + (style.lang || 'css')
      }
      this.loaderContext.callback(null, style.content, style.map as any)
      return
    }

    // custom
    if (query.type === 'custom' && query.index != null) {
      const block = this.descriptor.customBlocks[Number(query.index)]
      this.loaderContext.callback(null, block.content, block.map as any)
    }
  }

  // ---------------------------------------------------------------------------
  // proxy module code generation (script / template / styles / custom blocks)
  // ---------------------------------------------------------------------------

  private genScriptImport(): string {
    let scriptImport = `const script = {}`
    const { script, scriptSetup } = this.descriptor
    if (script || scriptSetup) {
      const lang = script?.lang || scriptSetup?.lang
      this.isTS = !!(lang && /tsx?/.test(lang))
      const externalQuery = Boolean(script && !scriptSetup && script.src)
        ? `&external`
        : ``
      const src = (script && !scriptSetup && script.src) || this.resourcePath
      const attrsQuery = this.attrsToQuery((scriptSetup || script)!.attrs, 'js')
      const query = `?vue&type=script${attrsQuery}${this.resourceQuery}${externalQuery}`

      let scriptRequest: string

      if (this.enableInlineMatchResource) {
        scriptRequest = this.stringifyRequest(
          genMatchResource(this.loaderContext, src, query, lang || 'js')
        )
      } else {
        scriptRequest = this.stringifyRequest(src + query)
      }

      scriptImport =
        `import script from ${scriptRequest}\n` +
        // support named exports
        `export * from ${scriptRequest}`
    }
    return scriptImport
  }

  private genTemplateImport(): string {
    const renderFnName = this.isServer ? `ssrRender` : `render`
    const useInlineTemplate = SFCLoader.canInlineTemplate(
      this.descriptor,
      this.isProduction
    )
    if (this.descriptor.template && !useInlineTemplate) {
      const src = this.descriptor.template.src || this.resourcePath
      const externalQuery = Boolean(this.descriptor.template.src) ? `&external` : ``
      const idQuery = `&id=${this.id}`
      const scopedQuery = this.hasScoped ? `&scoped=true` : ``
      const attrsQuery = this.attrsToQuery(this.descriptor.template.attrs)
      const tsQuery =
        this.options.enableTsInTemplate !== false && this.isTS ? `&ts=true` : ``
      const query = `?vue&type=template${idQuery}${scopedQuery}${tsQuery}${attrsQuery}${this.resourceQuery}${externalQuery}`

      if (this.enableInlineMatchResource) {
        this.templateRequest = this.stringifyRequest(
          genMatchResource(
            this.loaderContext,
            src,
            query,
            this.options.enableTsInTemplate !== false && this.isTS ? 'ts' : 'js'
          )
        )
      } else {
        this.templateRequest = this.stringifyRequest(src + query)
      }

      this.propsToAttach.push([renderFnName, renderFnName])
      return `import { ${renderFnName} } from ${this.templateRequest}`
    }
    return ``
  }

  private genStyleImports(): string {
    let stylesCode = ``
    let hasCSSModules = false
    const nonWhitespaceRE = /\S+/
    if (this.descriptor.styles.length) {
      this.descriptor.styles
        .filter((style) => style.src || nonWhitespaceRE.test(style.content))
        .forEach((style, i) => {
          const src = style.src || this.resourcePath
          const attrsQuery = this.attrsToQuery(style.attrs, 'css')
          const lang = String(style.attrs.lang || 'css')
          // make sure to only pass id when necessary so that we don't inject
          // duplicate tags when multiple components import the same css file
          const idQuery = !style.src || style.scoped ? `&id=${this.id}` : ``
          const inlineQuery = this.asCustomElement ? `&inline` : ``
          const externalQuery = Boolean(style.src) ? `&external` : ``
          const query = `?vue&type=style&index=${i}${idQuery}${inlineQuery}${attrsQuery}${this.resourceQuery}${externalQuery}`

          let styleRequest
          if (this.enableInlineMatchResource) {
            styleRequest = this.stringifyRequest(
              genMatchResource(this.loaderContext, src, query, lang)
            )
          } else {
            styleRequest = this.stringifyRequest(src + query)
          }

          if (style.module) {
            if (this.asCustomElement) {
              this.loaderContext.emitError(
                new Error(
                  `<style module> is not supported in custom element mode.`
                )
              )
            }
            if (!hasCSSModules) {
              stylesCode += `\nconst cssModules = {}`
              this.propsToAttach.push([`__cssModules`, `cssModules`])
              hasCSSModules = true
            }
            stylesCode += this.genCSSModulesCode(i, styleRequest, style.module)
          } else {
            if (this.asCustomElement) {
              stylesCode += `\nimport _style_${i} from ${styleRequest}`
            } else {
              stylesCode += `\nimport ${styleRequest}`
            }
          }
          // TODO SSR critical CSS collection
        })
      if (this.asCustomElement) {
        this.propsToAttach.push([
          `styles`,
          `[${this.descriptor.styles.map((_, i) => `_style_${i}`)}]`,
        ])
      }
    }
    return stylesCode
  }

  private genCustomBlocksCode(): string {
    let code = `\n/* custom blocks */\n`
    code +=
      this.descriptor
        .customBlocks!.map((block, i) => {
          const src = block.attrs.src || this.resourcePath
          const attrsQuery = this.attrsToQuery(block.attrs)
          const blockTypeQuery = `&blockType=${qs.escape(block.type)}`
          const issuerQuery = block.attrs.src
            ? `&issuerPath=${qs.escape(this.resourcePath)}`
            : ''

          const externalQuery = Boolean(block.attrs.src) ? `&external` : ``
          const query = `?vue&type=custom&index=${i}${blockTypeQuery}${issuerQuery}${attrsQuery}${this.resourceQuery}${externalQuery}`

          let customRequest

          if (this.enableInlineMatchResource) {
            customRequest = this.stringifyRequest(
              genMatchResource(
                this.loaderContext,
                src as string,
                query,
                block.attrs.lang as string
              )
            )
          } else {
            customRequest = this.stringifyRequest(src + query)
          }

          return (
            `import block${i} from ${customRequest}\n` +
            `if (typeof block${i} === 'function') block${i}(script)`
          )
        })
        .join(`\n`) + `\n`
    return code
  }

  private finalizeComponentCode(code: string): string {
    let out = code
    if (!this.propsToAttach.length) {
      out += `\n\nconst __exports__ = script;`
    } else {
      out += `\n\nimport exportComponent from ${this.stringifyRequest(
        exportHelperPath
      )}`
      out += `\nconst __exports__ = /*#__PURE__*/exportComponent(script, [${this.propsToAttach
        .map(([key, val]) => `['${key}',${val}]`)
        .join(',')}])`
    }

    if (this.needsHotReload) {
      out += this.genHotReloadCode()
    }

    out += `\n\nexport default __exports__`
    return out
  }

  // ---------------------------------------------------------------------------
  // absorbed code generation helpers
  // ---------------------------------------------------------------------------

  private genCSSModulesCode(
    index: number,
    request: string,
    moduleName: string | boolean
  ): string {
    const styleVar = `style${index}`
    let code = `\nimport ${styleVar} from ${request}`

    // inject variable
    const name = typeof moduleName === 'string' ? moduleName : '$style'
    code += `\ncssModules["${name}"] = ${styleVar}`

    if (this.needsHotReload) {
      code += `
if (module.hot) {
  module.hot.accept(${request}, () => {
    cssModules["${name}"] = ${styleVar}
    __VUE_HMR_RUNTIME__.rerender("${this.id}")
  })
}`
    }

    return code
  }

  // __VUE_HMR_RUNTIME__ is injected to global scope by @vue/runtime-core
  private genHotReloadCode(): string {
    const id = this.id
    const templateRequest = this.templateRequest
    return `
/* hot reload */
if (module.hot) {
  __exports__.__hmrId = "${id}"
  const api = __VUE_HMR_RUNTIME__
  module.hot.accept()
  if (!api.createRecord('${id}', __exports__)) {
    api.reload('${id}', __exports__)
  }
  ${templateRequest ? this.genTemplateHotReloadCode(id, templateRequest) : ''}
}
`
  }

  private genTemplateHotReloadCode(id: string, request: string) {
    return `
  module.hot.accept(${request}, () => {
    api.rerender('${id}', render)
  })
`
  }

  // these are built-in query parameters so should be ignored
  // if the user happen to add them as attrs
  private static ignoreList = ['id', 'index', 'src', 'type']

  private attrsToQuery(
    attrs: SFCBlock['attrs'],
    langFallback?: string
  ): string {
    let query = ``
    for (const name in attrs) {
      const value = attrs[name]
      if (!SFCLoader.ignoreList.includes(name)) {
        query += `&${qs.escape(name)}=${value ? qs.escape(String(value)) : ``}`
      }
    }
    if (langFallback && !(`lang` in attrs)) {
      query += `&lang=${langFallback}`
    }
    return query
  }

  private static hash(text: string): string {
    return crypto.createHash('sha256').update(text).digest('hex').substring(0, 8)
  }

  // ---------------------------------------------------------------------------
  // shared descriptor cache (used by the main loader and the block loaders)
  // ---------------------------------------------------------------------------

  static setDescriptor(filename: string, entry: SFCDescriptor) {
    SFCLoader.descriptorCache.set(SFCLoader.cleanQuery(filename), entry)
  }

  static getDescriptor(
    filename: string,
    compilerOptions?: CompilerOptions
  ): SFCDescriptor {
    filename = SFCLoader.cleanQuery(filename)
    if (SFCLoader.descriptorCache.has(filename)) {
      return SFCLoader.descriptorCache.get(filename)!
    }

    // This function should only be called after the descriptor has been
    // cached by the main loader.
    // If this is somehow called without a cache hit, it's probably due to sub
    // loaders being run in separate threads. The only way to deal with this is
    // to read from disk directly...
    const source = fs.readFileSync(filename, 'utf-8')
    const { descriptor } = parse(source, {
      filename,
      sourceMap: true,
      templateParseOptions: compilerOptions,
    })
    SFCLoader.descriptorCache.set(filename, descriptor)
    return descriptor
  }

  private static cleanQuery(str: string) {
    const i = str.indexOf('?')
    return i > 0 ? str.slice(0, i) : str
  }

  // ---------------------------------------------------------------------------
  // script resolution (compileScript + caching + type dependency bookkeeping)
  // ---------------------------------------------------------------------------

  /**
   * inline template mode can only be enabled if:
   * - is production (separate compilation needed for HMR during dev)
   * - template has no pre-processor (separate loader chain required)
   * - template is not using src
   */
  static canInlineTemplate(descriptor: SFCDescriptor, isProd: boolean) {
    const templateLang = descriptor.template && descriptor.template.lang
    const templateSrc = descriptor.template && descriptor.template.src
    return isProd && !!descriptor.scriptSetup && !templateLang && !templateSrc
  }

  static resolveScript(
    descriptor: SFCDescriptor,
    scopeId: string,
    options: VueLoaderOptions,
    loaderContext: LoaderContext<VueLoaderOptions>
  ) {
    if (!descriptor.script && !descriptor.scriptSetup) {
      return null
    }

    const isProd =
      loaderContext.mode === 'production' || process.env.NODE_ENV === 'production'
    const isServer = options.isServerBuild ?? loaderContext.target === 'node'
    const enableInline = SFCLoader.canInlineTemplate(descriptor, isProd)

    const cacheToUse = isServer ? SFCLoader.serverCache : SFCLoader.clientCache
    const cached = cacheToUse.get(descriptor)
    if (cached) {
      return cached
    }

    let resolved: SFCScriptBlock | null = null

    let templateCompiler: TemplateCompiler | undefined
    if (typeof options.compiler === 'string') {
      templateCompiler = require(options.compiler)
    } else {
      templateCompiler = options.compiler
    }

    try {
      resolved = compileScript(descriptor, {
        id: scopeId,
        isProd,
        inlineTemplate: enableInline,
        // @ts-ignore this has been removed in 3.4
        reactivityTransform: options.reactivityTransform,
        propsDestructure: options.propsDestructure,
        defineModel: options.defineModel,
        babelParserPlugins: options.babelParserPlugins,
        templateOptions: {
          ssr: isServer,
          compiler: templateCompiler,
          compilerOptions: {
            ...options.compilerOptions,
            ...resolveTemplateTSOptions(descriptor, options),
          },
          transformAssetUrls: options.transformAssetUrls || true,
        },
      })
    } catch (e) {
      loaderContext.emitError(e)
    }

    if (!isProd && resolved?.deps) {
      for (const [key, sfcs] of SFCLoader.typeDepToSFCMap) {
        if (sfcs.has(descriptor.filename) && !resolved.deps.includes(key)) {
          sfcs.delete(descriptor.filename)
          if (!sfcs.size) {
            SFCLoader.typeDepToSFCMap.delete(key)
          }
        }
      }

      for (const dep of resolved.deps) {
        const existingSet = SFCLoader.typeDepToSFCMap.get(dep)
        if (!existingSet) {
          SFCLoader.typeDepToSFCMap.set(dep, new Set([descriptor.filename]))
        } else {
          existingSet.add(descriptor.filename)
        }
      }
    }

    cacheToUse.set(descriptor, resolved)
    return resolved
  }

  // ---------------------------------------------------------------------------
  // error formatting (code frames for compiler errors)
  // ---------------------------------------------------------------------------

  static formatError(
    err: SyntaxError | CompilerError,
    source: string,
    file: string
  ) {
    const loc = (err as CompilerError).loc
    if (!loc) {
      return
    }
    const locString = `:${loc.start.line}:${loc.start.column}`
    const filePath = chalk.gray(`at ${file}${locString}`)
    const codeframe = generateCodeFrame(source, loc.start.offset, loc.end.offset)
    err.message = `\n${chalk.red(
      `VueCompilerError: ${err.message}`
    )}\n${filePath}\n${chalk.yellow(codeframe)}\n`
  }
}

export default function loader(
  this: LoaderContext<VueLoaderOptions>,
  source: string
) {
  return new SFCLoader(this, source).generate()
}
