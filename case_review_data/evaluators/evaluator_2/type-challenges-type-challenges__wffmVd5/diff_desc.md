# Injection design record — scattered supported-language registry in the publishing pipeline

## Maintenance motivation

The repository is a multi-language site: every challenge README exists in one default
language (`en`) and in translated variants (`zh-CN`, `ja`, `ko`, `pt-BR`). In the clean
state, one shared module — `scripts/locales.ts` — is the single place that says which
languages exist (`supportedLocales`), which one is the default (`defaultLocale`), and how
a localized artifact name is spelled for each of them (`f('README', locale, 'md')` →
`README.md` for the default language, `README.<locale>.md` otherwise). Every publishing
stage imports that module, so promoting a new translation language touches the roster in
one file (plus adding the translated content itself).

The realistic maintenance pain this case models: the roster is *conceptual* knowledge
("which languages does this site publish?") that every stage of a build pipeline needs, and
each stage consumes it in a slightly different way — iterating it, gating on it, offering
it as a menu, or turning it into a file-name infix. That heterogeneity is exactly what
invites each stage, over time, to stop deferring to the shared module and to keep a private
copy shaped for its own local convenience. The maintainer-visible symptom is then: adding
or renaming a supported language no longer means editing one module; it means finding every
stage that quietly re-lists the languages and remembering to update each one, in its own
local idiom — the classic shotgun-surgery experience, where a small conceptual change is a
scattered trail of small edits.

## Development evolution being modeled

The diff is shaped as accretion, not as one sweeping edit. Each publishing path receives the
copy that *most narrowly fits its own context*: the redirect builder gets a flat array of
route languages next to `build()`; the interactive playground generator gets a menu list;
the README renderer gets an infix table plus a small name helper; the disk loader gets an
infix table local to the function that probes sibling files (a natural "just here, for
these files" tweak); the translation publisher gets a tiny infix table right where it
spells its output path; the path-resolution helper swaps its conditional for a case
dispatch over the codes. Several tables carry explanatory comments in the voice of a
developer recording a local assumption ("Filename infix each language's variation carries
on disk."), which is how such copies typically get written down. The shared module itself is
untouched — its exports still exist; the pipeline just quietly stops consulting them for
roster and naming decisions.

## Overall design

Eight producers across `scripts/` each hold a private, complete declaration of the
supported language set, in four deliberately varied structural forms:

- one-line enum arrays (`['en', 'zh-CN', 'ja', 'ko', 'pt-BR']`), both at module scope and
  function scope;
- five-entry affix tables (`{ 'en': '', 'zh-CN': '.zh-CN', ... }`), both at module scope
  and function scope;
- a membership gate (`.includes(locale)`) feeding a conditional content choice;
- a case-dispatch `switch` over the language codes.

Where a stage's private copy decides *names*, a small local helper wraps the table
(`localizedInfix`/`publishedReadmeName`, `readmeFileName`) so the call sites read as if a
shared helper still existed. The knowledge kinds are spread across the surface: iteration
(build, README generation), naming (URL construction, README names, translation targets,
generic path resolution), membership (render eligibility), and selection (interactive menu).

## Changed locations

### `scripts/build.ts` — route and emission roster

What: the `supportedLocales` import is narrowed to `defaultLocale` (plus a type-only
import), and a module-scope constant `routeLocales: SupportedLocale[]` enumerates the five
languages with a comment ("Languages the short-link host serves routes for."). The three
consumption sites — the non-default redirect filter, the raw-README redirect loop, and the
per-quiz emission loop — now iterate the local constant.

Why this site and shape: `build()` is the pipeline stage that materializes the roster into
per-language outputs (redirects, emitted files). A module-scope flat array next to the
consumer mirrors how a build script grows a constants block "for the routes this stage
serves". Production role: decides which languages get short-link routes and which language
variants are emitted for every challenge.

### `scripts/readme.ts` — README name derivation and iteration roster

What: the `supportedLocales` and `f` imports are dropped; a module-scope table
`readmeLocaleInfixes` (plus a `readmeFileName(locale)` helper) spells the per-language file
infix. The available-language link list, the index-README loop, and the info-README loop
all switch to iterating `Object.keys(readmeLocaleInfixes)` or calling `readmeFileName`.

Why this site and shape: readme.ts is the stage with the most varied roster uses —
iteration *and* name formation *and* filtering for which translations actually exist — so
it earns the most "grown over years" shape: a key/value table whose keys double as the
iteration list, cast back to the locale type at each loop. Production role: regenerates
`README.<locale>.md` files and the available-language badges that link between them.

### `scripts/toUrl.ts` — published artifact name construction

What: a module-scope table `localeFileInfixes`, a `localizedInfix(locale)` lookup with a
fallback interpolation (`?? '.${locale}'`) for codes outside the table, and a
`publishedReadmeName(locale, localized)` wrapper replaces the previous default-locale
conditions inside the four README-URL builders, which now route through the helper with a
"localized" flag.

Why this site and shape: the builders each previously re-decided "default language keeps
the bare name" inline. Wrapping them around one local table-plus-helper is the refactoring
a developer does when these four builders start to feel repetitive — it centralizes the
naming locally *within* the module, while the pipeline-level knowledge still escapes. The
fallback interpolation preserves the original behavior for language codes that are not in
the roster, keeping URLs for hypothetical future in-between states identical to before.
Production role: produces every GitHub/jsDelivr/raw URL to localized READMEs that the site,
the redirects, and the info footers link to.

### `scripts/loader.ts` — per-language variation lookup on disk

What: inside `loadLocaleVariations`, a function-local table `variationInfixes` plus a loop
over `Object.keys(variationInfixes)` with the per-key infix interpolated into the sibling
filename, including the original `|| '.${locale}'` fallback for keys whose infix is the
empty string.

Why this site and shape: this stage reads sibling files on disk
(`README.md`, `README.zh-CN.md`, `quiz.zh-CN.ts`, ...), so its natural local need is "the
suffix on disk for each language". A function-local table is what a developer writes when
they only look at one function. The empty-string default for the default language forces
the subtle `||` fallback so the emitted filename stays exactly `.<locale>`-suffixed as
before, which keeps this copy structurally distinct from the module-scope tables.
Production role: loads every localized variation of READMEs, templates, tests, and info
files for each challenge.

### `scripts/translate.ts` — machine-translation target naming

What: the `resolveFilePath` import is dropped; inside `translateQuiz`, a tiny function-local
table `readmeInfixes` plus an inline template builds the destination README path
(`README${readmeInfixes[to]}.md`).

Why this site and shape: translation publishing writes one file for one target language, so
the smallest possible local copy suffices — a five-entry lookup used once. Its placement
inside the function makes it the most deeply hidden of the copies. Production role: decides
where a freshly machine-translated README is saved.

### `scripts/utils/resolve.ts` — generic localized path resolution

What: the two-branch conditional over `defaultLocale` becomes a `switch` dispatching on the
language codes: a bare case for the default, a shared case for the other four supported
codes, and a `default` arm reproducing the original conditional for any other code.

Why this site and shape: this helper is the residual "spare" implementation of localized
naming that other utility callers use; converting it to a case dispatch over the codes is
the shape a developer reaches for when they no longer want to reason about "default vs
rest" but about "which language is this". Keeping the `default` arm preserves behavior for
unknown codes. Production role: resolves localized file paths generically for callers that
have a directory, stem, extension, and language.

### `scripts/actions/utils/formatToCode.ts` — render eligibility gate

What: a function-local array `renderableLocales` and a membership test replace the
previous direct content selection: content is chosen only when the requested language is in
the local list, with the default language as the fallback.

Why this site and shape: this is the smallest and most incidental-looking copy — an
inclusion gate rather than an iteration, the form scatter takes when a stage only needs a
yes/no answer about a language. It is a different consumer style from every other site,
which is precisely why roster changes historically get missed here. Production role:
decides which language's content may be rendered into a generated playground file.

### `scripts/generate-play.ts` — interactive language menu

What: the `supportedLocales` import is replaced by a module-scope constant
`playgroundLocales` (with a type-only `SupportedLocale` import and a comment "Languages a
local playground can be generated for."); the CLI-argument match and the `prompts` select
menu both read from it.

Why this site and shape: the menu stage needs the roster twice in two different idioms
(find-by-argument and map-to-choices), so a module-scope list next to the only consumer is
the convenient local shape. Production role: decides which languages a maintainer can
generate a local playground for and what the interactive picker offers.

## Deliberate structural variation

The eight sites intentionally do not share one syntactic form, because a scattered registry
that accretes organically never does: two module-scope arrays, one function-scope array,
two module-scope affix tables, two function-scope affix tables (at two different
indentation levels), one membership gate, and one switch dispatch. Consequently the
duplicated knowledge can only be gathered by understanding every stage's *use* of the
languages — iterating, naming, gating, selecting, dispatching — and no single textual or
structural pattern spans the whole surface.
