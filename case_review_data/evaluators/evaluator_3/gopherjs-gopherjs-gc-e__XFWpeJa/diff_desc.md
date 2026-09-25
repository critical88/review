# Injection design record — per-package assembly centralization in GopherJS

## Maintenance motivation

GopherJS's per-package build flow is a relay between several cooperating pieces:

- the build **session**, which drives the overall build and sequences what happens to each package;
- the **package descriptor** produced by analyzing a package, which carries the facts other stages ask about;
- the **native-overlay augmentation stage**, which parses original and overlaid sources and rewrites parse trees so that vendored versions of the standard library and other specialized packages compile in the browser environment;
- **embedded-asset synthesis**, which generates the initializer source for packages using `go:embed`;
- **descriptor metadata** accessors (source modification time, installed archive path) and the construction of per-package **variants** (in-package test package, external test package, a fabricated ad-hoc descriptor for synthetic targets).

Overlay handling is the piece that has grown the most, because every release adds new arch-tag special cases, override directives (`gopherjs:new`, `gopherjs:purge`, `gopherjs:override-signature`), and pruning rules. Working in that stage means threading per-package context (the pending override bookkeeping, the matched-names set) through every step, because today that state is built and passed along explicitly by the sweep function.

The motivating attitude modeled here is the one that shows up on real issue trackers for build systems: *the session already sees every package, so stop passing this state around — let the session own the whole per-package assembly.* That is an attractive simplification in the small and a well-known trap in the large, which is precisely why this evolution is worth modeling.

## Normal development evolution being modeled

The diff models a short sequence of "tidy-up" pull requests by one author, each defensible on its own:

1. **Centralize the per-package assembly state.** Rather than passing the override bookkeeping (`overrides`, `found`) through the augmentation family as parameters, store them on the build session that already drives the sweep, and reset them at the start of each package. To keep the call shape uniform, the whole augmentation family migrates onto the session receiver and moves into a source file dedicated to that stage. The package's tests are adjusted to drive the stage through the session, since that is now the only entry point.
2. **Finish the one-driver cleanup.** With the session already the home of the per-package state, the same pull applies to the remaining package questions the session used to *ask* other owners: embedded-asset synthesis moves onto the session, and the descriptor's variant construction (in-package test variant, external test variant) and its metadata answers (source modification time, installed archive path) are re-owned as session methods so the per-package flow never consults another owner for package facts. The CLI's install-path lookup follows, because package questions now go through the session; the ephemeral ad-hoc descriptor used for synthetic targets is extracted to sit beside its siblings in the new descriptor file, sharing the session receiver.

Nothing here changes user-visible behavior: the CLI flags, the environment handling, the directive semantics, the produced initializer source, the variant tags, and the artifact layout are all preserved. What changes is *where* the per-pipeline knowledge lives and *who* answers questions about a package.

## Overall design

The sequence lands as six files:

- `build/augment.go` (new): the overlay augmentation stage — parsing of overlaid and original sources, directive-driven rewriting, override bookkeeping and validation, import pruning, and removal finalization — as a family of session-owned callables, together with the `overrideInfo` descriptor type that supports them.
- `build/pkgmeta.go` (new): the package-variant construction and descriptor-metadata callables re-owned from the descriptor type and from the inline assembly in the per-package flow, including the ad-hoc ephemeral descriptor fabrication.
- `build/build.go`: the session type gains two per-package state fields for the augmentation stage and its documentation is updated to describe the session's expanded role; the per-package flow's call sites are rewired; the session's import surface takes over the machinery imports the moved stages need; the descriptor keeps only what still belongs to it.
- `build/embed.go`: the embedded-asset synthesis callable joins the session-owned family.
- `build/build_test.go`: the tests that exercise augmentation and descriptor variants switch to driving the new owners; their scenarios and assertions are otherwise untouched.
- `tool.go`: the CLI's install-path question is routed through the session, consistent with the rest of the sequence.

## Per-cluster rationale

### Native overlay augmentation with per-package override state

**What changed.** The augmentation sweep and its supporting steps (overlay parsing, original-source parsing, per-file overlay rewriting, original-import augmentation, original-file augmentation, override validation, imports-only detection, import pruning, removal finalization) moved from package-level callables taking an explicit build context and explicit state parameters onto the session receiver. The `overrideInfo` type declaration accompanies them into the stage's own file. The session gains two fields that together hold the per-package augmentation state — the pending override descriptors by name and the set of names that overrides actually matched — and the sweep resets both at the top of each package, which is what keeps the documented state-transition behavior identical (the state is per-package, never carried across packages).

**Why this site and shape.** The stage's own file keeps the augmentation logic findable next to the vendored-overlay machinery, and receiver-style entry points let every step consult the shared state without the parameter threading that motivated the change. The inner declaration-rewrite helpers get a plain local receiver-variable name change to avoid shadowing the session receiver inside the type switches — a mechanical consequence of moving code whose bodies previously named their parameters `s`.

**Production role.** This stage is on the critical path for building anything with vendored or arch-tag special casing: every package built for the browser passes through it, and the directive semantics (`gopherjs:new`, `gopherjs:purge`, `gopherjs:override-signature`) are covered by dedicated test scenarios that drive it.

### Embedded-asset initializer synthesis

**What changed.** The callable that resolves `go:embed` patterns and emits the `js_embed.go` initializer (for all four embed kinds: alias, bytes, string, and the filesystem table) moved onto the session receiver beside the augmentation family.

**Why this site and shape.** In the modeled narrative, this move rides along with the second pull request's "the session is where per-package work happens now" theme, and it buys uniformity: everything that produces per-package synthetic source answers to the same owner. Its state needs remain (honestly) just the package descriptor and file set, which the receiver still receives explicitly.

**Production role.** Any package using `go:embed` gets its initializer source built here before compilation; the available embed kinds must keep resolving to the same initializer text.

### Package-descriptor variants and metadata

**What changed.** The descriptor's value-receiver methods for the in-package and external test variants, the source-modification-time lookup, and the installed-archive path resolution were re-owned as session methods, now taking the descriptor they operate on as a parameter; the ad-hoc ephemeral descriptor fabrication that used to live inline in the per-package flow moved into the new descriptor file with the same receiver, alongside its siblings.

**Why this site and shape.** The modeled author's stated intent is that questions about a package should not scatter across types the session merely coordinates; in this reading, the descriptor stays a data record while the session becomes the place that knows how variants of it are built and which facts about it matter during a build. The metadata callables sit in the same new file as the variant constructions because they belong to the same "package facts" cluster in the author's mental model. The ad-hoc fabrication keeps its flow context (forced build attributes, patched dummy timestamp, joined embedded patterns) exactly as before, only relocated.

**Production role.** Building tests (internal and external) and building synthetic tooling targets depend on these variant constructions; the CLI asks the same cluster where an installed archive lives.

### Call sites, tests, and import surface

**What changed.** The per-package flow's call sites route through the session (the sweep, the embed step, the test-variant loads, the test-package flow); the test that walks per-package variants drives the new accessors; the CLI's install-path lookup follows the same owner; the session's import block absorbs the machinery imports the moved stages need (`go/parser`, the scanned-position helpers, directive-scanning helpers) and drops what now lives elsewhere; the moved test code invokes the stage through the session.

**Why.** This is the mechanical continuation of the re-homing: after the receiver change, every caller's question goes to the same place, and build configurations that the moved callables used to consult through parameters now arrive through the session's build context.

**Continuity detail.** One accessor that internal packages consume stays on the descriptor deliberately (the internal build context it exposes), because the internal consumers keep their existing dependency; everything else about the descriptor's outward surface is unchanged.

## Why this is a realistic GopherJS change

Every piece of the modeled sequence appears in real maintenance history of build tools: state hoisting onto the orchestrator, receiver-uniformity cleanups, "one place that answers questions about X" refactors, and callers following along mechanically. The repository's overlay machinery is exactly the place such growth hurts, because its stage previously held its state locally and passed it explicitly — the property that keeps augmentation debuggable even while the vendored-overlay surface keeps expanding.

## Continuity commitments in the diff

The sequence is behavior-preserving by construction: command-line surface, environment handling, directive semantics, initializer output, variant tags and contexts, artifact layout, and the descriptor internals consumed by internal packages are untouched, and every moved body is relocated verbatim apart from receiving the new receiver/leading-argument forms. The regression scenarios covering override directives, imports-only files, pruning, removals, and variant walks continue to describe the same expected outcomes, now driven through the new owners.
