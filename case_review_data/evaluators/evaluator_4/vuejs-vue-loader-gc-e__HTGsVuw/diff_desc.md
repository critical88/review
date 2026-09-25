# Injection design record — vue-loader SFC compilation consolidation (`god_classes`)

Task: `vuejs-vue-loader-gc-e` · Repository: vuejs/vue-loader @
`698636508e08f5379a57eaf086b5ff533af8e051` · Smell: `god_classes` ·
Diff: 10 files, 673 insertions, 567 deletions, 18 hunks, 1462 patch lines.

This record documents what the injected diff changes, why each site and shape was
chosen, and the production role every change serves. It is an auditable design
rationale for the change itself; it does not prescribe any particular follow-up work.

---

## 1. Maintenance motivation being modeled

vue-loader processes a `.vue` request in two very different regimes that share state:

- the **main request** parses the component, caches the descriptor, resolves the
  script, and emits the proxy module that imports the language blocks;
- the **block requests** (`?type=template|style|...`) and the pitcher-injected
  **template loader** need the same parsed descriptor, the same resolved script, and
  the same compiled-result caches — and the **webpack plugins** (both the webpack 4 and
  the webpack 5 variant) must observe the shared caches on every rebuild to invalidate
  compiled results when an imported source file or a type dependency changes.

At the pinned revision that sharing is spread over several small modules
(`descriptorCache.ts`, `resolveScript.ts`, `select.ts`, `cssModules.ts`,
`hotReload.ts`, `formatError.ts`) while the request-level flow lives in one long
`loader()` function in `src/index.ts` whose intermediate state is a row of closure
locals passed to those helpers by hand.

The evolution this diff models is a plausible maintainer-driven consolidation step:
a contributions freeze period where the team reshapes the entry into a single
class-based coordinator so that

1. the per-request working state becomes explicit fields on an object instead of
   long rows of closure locals threaded through free functions, which repeatedly caused
   "which values are actually live at this point" review mistakes;
2. all shared, process-wide compilation state (descriptor cache, client/server script
   caches, imported-type dependency map, one-shot warning flag) becomes addressable
   through one type that companion modules can import — the webpack 5 plugin's
   rebuild hooks and the template loader already import from the entry module, so an
   entry-owned class is the lowest-friction consolidation point available without
   introducing a new shared-state package;
3. every phase of a request (parse → cache → select → generate) is a named method on a
   self-contained object, which reads as good local structure and is greppable in
   stack traces.

This is exactly the kind of step real teams take with the best local intentions: each
individual move is defensible, the resulting shape concentrates many responsibilities
that change for different reasons into one owner, and consumers converge on it as the
mandatory hub. The maintenance story is what makes the consolidation believable —
reviewers had asked for the request flow to be "one place", and this is the cheapest
reading of that request.

## 2. Evolution modeled

The diff models normal incremental growth, not a single dramatic edit:

- The entry is reshaped from a monolithic exported function into a default-exported
  function wrapper plus an exported coordinator class; the wrapper preserves the
  webpack-visible `LoaderDefinitionFunction` interface (callers obtain the component
  import list, not an object).
- Six existing cohesive modules are absorbed one at a time, each absorption motivated
  by state it shares with the request flow (details per location below). The public
  functions those modules exported are converted into static members of the same class
  so the three importing consumers keep compiling with a mechanical import rewire.
- Per-request values become instance fields assigned in the constructor and consumed by
  phase methods; the method decomposition mirrors the original statement order of the
  old loader body, so the emitted proxy-module code is preserved line for line.

Behavior is fully preserved by construction: every method returns the same strings the
old helper functions produced, phase order is unchanged (script import, template
import, styles, custom blocks, finalize, hot reload), early returns (error path and
language-block dispatch) keep their shapes, and the module caches are the same objects
with the same lifetimes. The emission is verifiable by diffing the emitted module
fragments before/after on the repo's own test fixtures (the Jest suite asserts on
emitted code extensively).

## 3. Overall design of the injected change

`src/index.ts` after the change:

```text
export default function loader(this: LoaderContext, source: string) {
  return new SFCLoader(this, source).generate()
}

export class SFCLoader {
  // process-wide shared state (module-wide caches reached by consumers)
  static descriptorCache          // Map<clean filename, descriptor>
  static clientCache / serverCache // WeakMap<descriptor, compiled script>
  static typeDepToSFCMap          // Map<imported file, Set<sfc file>> (plugin HMR)
  static errorEmitted             // one-shot missing-plugin warning flag

  // per-request state
  loaderContext, source, options, sourceMap, rootContext, resourcePath, ...
  incomingQuery, resourceQuery, enableInlineMatchResource, isServer, isProduction,
  filename, asCustomElement, descriptor, errors, rawShortFilePath, shortFilePath,
  id, hasScoped, needsHotReload, isTS, templateRequest, propsToAttach

  constructor(loaderContext, source)   // extracts request facts
  generate(): string | undefined       // orchestrates the phases in order

  // phases (instance)
  ensurePluginInstalled()  loadSFC()  cacheDescriptor()  computeScopeId()
  stringifyRequest()      selectBlock()                // early-return dispatch
  genScriptImport()  genTemplateImport()  genStyleImports()  genCustomBlocksCode()
  finalizeComponentCode()  genCSSModulesCode()  genHotReloadCode()
  genTemplateHotReloadCode()  attrsToQuery()

  // absorbed shared helpers (static: descriptor get/set, script resolution,
  // inline-template decision, error formatting) + private statics (hash, cleanQuery)
}
```

`generate()` performs the same sequence the old exported function did: guard the plugin
installation, parse and cache the descriptor (emitting formatted errors and returning
an empty module on failures), compute the scope id, dispatch language-block requests
early through the absorbed selection logic, then build the script import, the template
import, the style imports (with CSS-modules wiring), the custom blocks code, attach the
runtime properties, and append the absorbed hot-reload code.

Consumers are rewired mechanically:

- `src/templateLoader.ts` now imports the class from the entry and calls its static
  descriptor lookup, script resolution and error formatting
  (3 references — replacing `formatError`, `getDescriptor`, `resolveScript` imports);
- `src/pluginWebpack4.ts` and `src/pluginWebpack5.ts` import the class and reach the
  shared caches through it in their rebuild/invalidation hooks
  (8 state references: `typeDepToSFCMap`, `descriptorCache`, `clientCache`).

No consumer's own logic changes: the endpoint of every shared-state access is the same
cache object that the absorbed modules previously owned.

## 4. Per-location rationale

### `src/index.ts` — entry reshaped into the coordinator class (+659/−277)

**What changed.** The exported `loader` function is reduced to a three-line default
export that constructs the coordinator and calls one method. The request-flow body is
distributed across a constructor plus 17 instance/static methods; per-request locals
become 24 instance fields; the absorbed modules' shared singletons become 5 static state fields; the
absorbed helpers from the six deleted modules become instance or static methods.

**Why this site.** The entry is where the request flow already lived, it is already the
module every consumer imports (`templateLoader.ts` imports `VueLoaderOptions` from
`'./'`; both plugins import the entry through `./plugin`), and it is the only place in
the package from which one type can be exported to all peers without new filesystem
layout. It is the natural seat for a "one object owns the request" refactor.

**Why this shape.** (a) A wrapper default export preserves webpack's loader contract
(loaders are called as functions) while allowing the internals to be an object — so the
public boundary never changes during the refactor. (b) The phase decomposition keeps
statement order identical to the old function body, which makes emission equality
reviewable line by line. (c) Statics (not a companion singleton module) keep the shared
state next to the methods that use it and keep every peer importable from one path.
(d) The constructor eagerly extracts request facts (`isWebpack5`, `isServer`,
`isProduction`, filename/queries) so each phase method reads as domain logic, not
webpack plumbing — at the cost of a large field set that the entry class now has to
carry alone, which is what concentrates so many different concerns in one place (query
shape from webpack, parser behavior, per-block string generation, error presentation).

**Production role.** Same as before the change: main entry of the loader; additionally
the export point of the shared compilation state and helpers that peers depend on
during rebuilds and template compilation.

### `src/select.ts` — deleted, absorbed as `selectBlock` (−52)

**What changed.** Language-block selection moves into the coordinator as an instance
method that reads the descriptor, the scope id, the options, the parsed incoming query
and the loader context fields (all request state the entry owns), preserving the early
return through the block callback. The module's own import of the descriptor cache is
gone — the class reaches its own static caches.

**Why this site.** Selection is the first branch of every main request, and after the
consolidation it reads and writes the same per-request fields as the generation phases.
**Why this shape.** Keeping it a method of the request object removes the parameter
bundle the old free function took (descriptor, id, options, context, query,
appendExtension) — the request object now substitutes for that whole signature.
**Role.** Dispatches `?type=` requests to the proper block loader and returns early.

### `src/cssModules.ts` — deleted, absorbed as `genCSSModulesCode` (−26)

**What changed.** The CSS-modules fragment generation becomes a private instance method
that reads the already-computed scope id, block index and request string from the
request fields and the request-wide hot-reload flag. **Why this site.** It is invoked
from inside the style loop and only by it. **Why this shape.** As a method it no longer
needs its four parameters threaded by name through the loop; it consumes the same
values without re-passing them. **Role.** Emits the per-module-name exported getter
glue for `<style module>` including the hot-reload rerender branch — string-identical
to before.

### `src/hotReload.ts` — deleted, absorbed as `genHotReloadCode` / `genTemplateHotReloadCode` (−27)

**What changed.** The two exported HMR codegen helpers become instance methods; the
entry keeps the template request string on a field so the hot-reload fragment can be
produced at finalization time. **Why this site.** HMR codegen depended on the scope id
and on the template request the entry had already built — values now carried by the
object. **Why this shape.** Splitting the record-registration from the template accept
callback mirrors the two existing files-level concerns and keeps the emitted fragment
byte-identical. **Role.** Emits the hot reload API client registration that the Vue
runtime uses to swap components without page reloads.

### `src/formatError.ts` — deleted, absorbed as a static + call sites inlined (−22)

**What changed.** Compiler-error formatting becomes a static class method used both by
the main flow and externally by the template loader; its module-scope helper moves with
it. **Why this site.** Two consumers (entry parse errors, template compile errors)
already shared it; consolidating shared helpers for loaders is the stated goal of the
change. **Why this shape.** Static, because it touches no request state and template
loader calls it before any request object exists. **Role.** Turns a thrown compiler
error into a readable code frame on the console.

### `src/descriptorCache.ts` — deleted, absorbed as `descriptorCache` statics (−39)

**What changed.** The filename-keyed descriptor cache and its `setDescriptor` /
`getDescriptor` accessors (with the fs fallback for not-yet-cached filenames) become a
static field plus static methods on the class. **Why this site.** The entry creates the
descriptor on every main request, three other flows read it, and making the entry the
addressable owner is the consolidation goal. **Why this shape.** The cleaned-query
keying method moves with it privately; the fallback parse stays in the getter so the
module-identity semantics are untouched. **Role.** Guarantees one shared parsed
descriptor per file across main, block and template requests.

### `src/resolveScript.ts` — deleted, absorbed as `resolveScript` + `canInlineTemplate` statics (−105)

**What changed.** Script resolution — the client and server compiled-script caches, the
`compileScript` invocation with its options mapping, the imported-type dependency
mapping used by the plugins' watch passes, and the inline-template decision — becomes
static methods over static cache fields on the class. `canInlineTemplate` moves because
it was exported by the same module (script resolution being the only consumer of the
template-options mapping helpers). **Why this site.** It holds the second-largest
shared-state cluster in the flow and the one the plugins are most sensitive to. **Why
this shape.** Statics preserve the call form already used at both call sites; the cache
emptiness proof and dependency-mapping bookkeeping keep their original semantics
verbatim. **Role.** Produces (and caches) the compiled script binding metadata for the
main emission and templates and records which type-level dependencies each SFC imports
so rebuilds can invalidate precisely.

### `src/templateLoader.ts` — imports rewired (+4/−7)

**What changed.** Only the imports and the shared-resource endpoints: descriptor
lookup, script resolution and error formatting are now `SFCLoader.` static calls; the
loader's own logic is untouched. **Why this site.** It is one of the three external
consumers that import the entry already (`VueLoaderOptions` comes from `'./'`), so the
mechanical rewire is one line per helper and requires no design decision.
**Role.** Compiles templates using the shared descriptor and resolved script — the
proof that shared state stays shared.

### `src/pluginWebpack4.ts` / `src/pluginWebpack5.ts` — imports rewired (+5/−6 each)

**What changed.** The rebuild hooks stop reaching the caches through their modules and
reach them through the imported entry class; the per-variant rule cloning and watch
logic is untouched. **Why this site.** The plugins already depend on the entry module
(each plugin variant lives behind `./plugin` importing per webpack version), so the
state access becomes a class-member access on an import they already hold. **Role.**
On rebuild, find components invalidated by changed type-level dependencies, clear their
compiled results, and collect affected SFC source files — the webpack 4 and webpack 5
expressions of the same responsibility.

### What the diff does not touch

`pitcher.ts` (request rewriting over the loader chain), `stylePostLoader.ts` /
`styleInlineLoader.ts` (leaf style loaders), `util.ts` (getOptions, request
stringification and match-resource helpers used symmetrically by many modules),
`compiler.ts` / `plugin.ts` (dispatch shims), and the
tests. The consolidation deliberately models the smallest cross-coupling step a team
under deadline pressure actually ships — not a maximal reorganization.

## 5. Structural properties produced

Worth recording as the design's structural consequences (visible in `smell.diff`):

- One class owns the entire request lifecycle plus the shared caches (24 members count
  as methods; 24 per-request fields; 6 static fields, of which the 4 caches (descriptor
  cache, client/server script caches, imported-type dependency map) are process-wide and
  shared with consumers).
- Consumers of three flavors (block-loading, template compilation, both plugin
  variants) import the single class for their shared state and helpers — the entry
  becomes the package's mandatory hub.
- The class's methods cluster into several disjoint responsibility groups (parse/cache,
  selection, per-block codegen, CSS modules, HMR, error presentation, request-query
  plumbing) with little overlap in the fields they touch — each group changes on its
  own schedule (compiler features vs webpack request shape vs runtime reload shape vs
  diagnostics).

These properties are what the change's own maintainers would later have to reason
about whenever any part of SFC handling evolves or misbehaves in production.
