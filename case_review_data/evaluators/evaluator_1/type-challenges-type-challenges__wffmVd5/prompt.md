# Consolidate the supported-language registry in the publishing pipeline

## Repository context

This repository publishes every challenge in several site languages. One module,
`scripts/locales.ts`, was written as the single source of truth for locale decisions: it
declares the `SupportedLocale` codes the site publishes, which language is the default,
the translation lookup used by rendering helpers, and the rule for how a localized
artifact is named (the default language keeps the bare file name, every other language
carries its own code as an infix — `README.md`, `README.zh-CN.md`, `quiz.ja.ts`, and so
on).

The `scripts/` directory is the publishing pipeline that turns the `questions/` corpus and
its per-language variations into site artifacts: short-link redirects and emitted files
for every published language, regenerated challenge READMEs and their available-language
links, generated playground files, and machine-translation write targets.

## Maintainer observation

When we last brought a new translation language up to a full site experience, the work
did not stop at adding translated content — we had to hunt through the publishing
pipeline and update stage after stage that had quietly grown its own idea of which
languages exist: one stage had its own list of route/emission languages, README
generation iterated its own language table and spelled localized file names from its own
infix data, the URL builders derived localized names from their own mapping, the
translation flow spelled its output file name from its own local knowledge, the shared
path-resolution helper had learned to dispatch on the language codes themselves, and the
playground menu and the render gate each kept their own copy of the language list.

The architectural diagnosis we have settled on: `scripts/locales.ts` still *exports* the
language roster and the localized-naming decision, but the pipeline no longer consumes
them for those purposes — each stage re-declares the complete set (and re-derives the
naming rule) in whatever local shape suited its own code. The knowledge "which languages
does this site publish, and how are their artifacts named" is no longer owned by one
module; it is scattered across the publishing stages, so any conceptual change to the
language set forces a coordinated trail of small edits all over `scripts/` — the
shotgun-surgery pattern.

## What we want

Restore single ownership of the supported-language decision:

1. The complete supported-language roster, and every rule derived from it that other
   modules need (in particular how localized artifact names are formed), is knowledge
   that lives in the shared locale module — the pipeline's stages must obtain it from
   there, not keep private re-declarations.
2. No publishing stage keeps its own complete enumeration of the supported languages or
   its own copy of the localized-naming rule; a maintainer adding (or renaming) a
   supported language touches the shared module and adds the translated content — no
   other stage needs to be told the news.
3. Existing behavior must be preserved exactly; in particular every fact listed under
   "What must not change" below still holds after the change.

This is a restructuring of how the pipeline learns about languages, not a change to
what it publishes.

## What must not change

- The localized artifact naming scheme is untouched: the default language keeps the bare
  file name (`README.md`), every other supported language carries its own code as an
  infix (`README.<locale>.md`, `README.zh-CN.md`), with the same `.<locale>` interpolation
  as today for any language code outside the published set where the current code
  interpolates one.
- The published artifact set is byte-identical: the short-link redirect table produced by
  the site build, the generated challenge READMEs and the links between language variants,
  the emitted per-language files, playground generation and the generated playground
  files, and the paths machine-translated READMEs are written to.
- The interactive flows behave the same: selecting a language by command-line argument or
  from the menu in playground generation, and the fallback to the default language when a
  requested language has no content.
- Modules keep working together: callers of the shared helpers continue to compile and run
  against them; do not break other callers as part of this change.
- The repository's TypeScript challenge suite and the `utils/` package remain green.

## Scope and deliverable

- Do not add or remove any actual language, change translated content, or modify the
  `questions/` corpus — the published behavior for the current set of languages is
  exactly what you must keep.
- Produce a patch that restructures the pipeline to consume the supported-language
  knowledge from its shared owner, and removes the per-stage copies of it.

## Working notes

The pipeline is exercised with `pnpm build` (after `pnpm install --frozen-lockfile`);
the repository's own test suite is the type-level challenge tests validated with
`pnpm exec tsc --noEmit -p utils/tsconfig.json`. Both should succeed before and after
your change.
