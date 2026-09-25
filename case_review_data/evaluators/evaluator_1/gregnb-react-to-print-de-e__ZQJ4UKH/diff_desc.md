# Injection design record — dormant legacy print-flow surface

Task `gregnb-react-to-print-de-e` over `react-to-print` at `e17cb294f776e98e68ab9a469d52ee3396cb1e6f`.
Diff shape: 10 production files, 13 hunks, 287 insertions, 1 deletion; three of the files are entirely new.

## Maintenance motivation and modeled history

`react-to-print` 3.0.0 was a breaking release: it removed the component API
(`ReactToPrint`, its context consumer, the `trigger` prop), renamed `content` to
`contentRef`, and renamed the options interface to `UseReactToPrintOptions`.
Migrations of that size never land in one move. A realistic transitional
pattern for a library with a large installed base is to keep the replaced
machinery behind a build-time escape hatch for a release or two — "pin your
version and define one constant" — then drop it once telemetry says the
pre-3.0 users are gone. The common failure of that pattern is that the drop
never happens: the escape hatch is only ever *declared*, nothing in the
package actually defines it, the guarded paths accumulate supportive helpers,
and the whole limb survives release after release because every file looks
individually reasonable and the gates stay green.

This diff models exactly that late stage of the evolution: the escape-hatch
constants, the remnant branches of the replaced flow at each pipeline stage
where the old and new behavior diverged, and the handful of helpers, entire
modules, and identity constants that by then existed only for the dormant limb.
The 3.3.0 line and the unreleased iframe sizing work (`printIframeProps`
width/height) supply the second thread of the story: when the sizing feature
was being drafted, sizing a print window from a reference element's bounding
box was a real candidate implementation. That draft left behind a resolver and
a fallback entry point that the shipped feature does not use.

## Overall design

One ambient build-time switch is the anchor: `REACT_TO_PRINT_LEGACY_FLOW`,
declared (only) in a new declaration file as `boolean | undefined`, with no
runtime definition anywhere in the package, its build, or its examples.
Surrounding that anchor, the dormant limb spans every stage of the print
pipeline:

- five guarded branches inside live pipeline functions, one at each stage
  where the pre-3.0 flow genuinely behaved differently from the hook flow;
- two whole modules that only that flow (or nothing at all) imports;
- three helper declarations added inside otherwise-live modules, each
  explained by the same migration fiction;
- two identity constants added to the shared constants module;
- the switch's own declaration file.

The dependency direction was kept deliberately one-way: dormant code imports
live code, never the reverse. Every live function therefore compiles and
behaves exactly as in the pinned tree, and each dormant piece is justified by
a believable production role instead of by an arbitrary marker. The guarded
statements reference the switch exactly the way a real bundler-gated branch
would, which keeps them type-consistent under the project's strict
type-checked settings and its compiler options (`allowUnreachableCode: false`,
`noUnusedLocals`, `noUnusedParameters`).

## Changed locations and clusters

### 1. The escape-hatch declaration (new file: `src/types/legacyEnvironment.d.ts`)

A single `declare const REACT_TO_PRINT_LEGACY_FLOW: boolean | undefined` with
a JSDoc explaining that distributors of *pinned builds* can re-enable the
pre-v3.0 print handling by defining the constant through their bundler, and
that the published package never defines it.

*Why this shape:* an ambient declaration is the standard way a library types a
compile-time define without shipping a value. It makes the guards below look
data-dependent, keeps the type-checker satisfied, and matches how real
"legacy flow behind a bundler define" code reads.
*Production role:* the compatibility contract of the migration window; the
one place that documents the dormant feature's existence.

### 2. Hook orchestration (`src/hooks/useReactToPrint.ts`, inside `beginPrint`)

A guarded block that emits a debug-level log line announcing the start of a
"legacy print flow", placed just before the print window is appended.

*Why this site:* `beginPrint` is the phase coordinator and the busiest
function in the package, and an observability-only branch is the cheapest
realistic remnant: the old flow emitted per-stage diagnostics its consumers'
tooling depended on, so a migration-minded author would keep the trace point.
*Role:* legacy observability hook.

### 3. Print start and cleanup (`src/utils/startPrint.ts`, inside the `setTimeout` callback of `startPrint`)

A guarded block (placed after `handleAfterPrint` is declared, before the
`if (print)` custom-function branch) that prints synchronously from the
mounted window and defers cleanup by a fixed delay.

*Why this site:* the component-era flow was driven by the mounted hidden
window and could not rely on the dialog's lifecycle events, so a fixed
`setTimeout` cleanup was its documented workaround; this is the pipeline
stage where old and new behavior diverge most visibly.
*Role:* the alternate print-driving strategy of the replaced flow. The block
returns early, so the dormant limb genuinely owns a whole execution strategy.

### 4. Content-resolution fallback (`src/utils/getContentNode.ts`, inside `getContentNode`)

A guarded block, before the `if (contentRef)` resolution, that looks up a
well-known root element by the shared legacy id and returns it when present.
This is the only modified site that *consumes a shared constant added for the
limb* (`LEGACY_ROOT_ID`, cluster 7), so the dormant branch has a cross-file
supply chain.

*Why this site:* under the component API, triggering elements were allowed to
mount print content under one well-known root so several triggers shared one
target regardless of the refs they held; that contract had to be honored while
those builds existed.
*Role:* the alternate content source of the replaced flow, and the case's
clearest cross-file cleanup coupling (branch in one file, constant in another).

### 5. Shadow-DOM cloning fallback (`src/utils/clone.ts`, inside `cloneShadowRoots`; new private helper `collectLegacyShadowRootTemplates`)

A guarded block after the source/target size-mismatch early return that, in
the old flow, collected declarative `<template shadowroot>` hosts and logged a
warning per re-attached host. The collection logic lives in a new
module-scope, non-exported helper at the bottom of the file.

*Why this site:* before `element.shadowRoot` became broadly available,
declarative template markup was the only way to expose shadow content, so the
legacy flow kept a template-based fallback. The helper is deliberately
*private* (no export): it is the case's instance of a module-scope function
whose only caller is a branch that cannot run.
*Role:* the alternate shadow-content mechanism; a private helper variant of
the dead-helper shape.

### 6. Interactive-state transfer (`src/utils/handlePrintWindowOnLoad.ts`, inside `handlePrintWindowOnLoad`; new exported helper `applyClonedElementState`)

A guarded block just before the live "// Copy select states" loop, and a new
exported function at the end of the file that copies select, textarea, and
input state in one pass.

*Why this site:* the workflow split — one pass that copied all interactive
state, versus per-kind copying where each element type is loaded — was a real
internal restructuring during the 3.x work, and a "kept for the legacy flow"
single-pass entry point is precisely the kind of transitional export a
migration leaves behind. The block sits directly beside the live per-kind
copy, which is where the two designs actually contest.
*Role:* the old state-transfer path of the same responsibility the live code
performs; an *exported* helper variant of the dead-helper shape (the export
legitimizes its survival through a public-looking surface).

### 7. Shared identity constants (`src/consts.ts`)

Two new exported constants beside the live `DEFAULT_PRINT_WINDOW_ID`: the id
of the legacy flow's well-known content root, and the id given to the print
window owned by the legacy flow's session manager.

*Why this site:* id-based DOM lookup needs shared identifiers; module-scope
constants far from their consumers are the classic survivor of any feature
removal because they look like harmless data.
*Role:* DOM addressing for the dormant limb (consumed by clusters 4 and 8).

### 8. Print window session manager (new file: `src/utils/printWindowManager.ts`)

An entire module: an exported `PrintWindowManager` class holding a
`managedWindow`/`referenceElement` pair (register-reference, acquire — reusing
the live window generator and assigning the managed-window id — release, and
abort with a diagnostic log), a module-level singleton, and an exported
accessor returning it.

*Why this shape:* multi-print sessions that reused one mounted window were a
real trait of the pre-v3.0 behavior, and its owner is exactly the kind of
self-contained module a migration isolates. It imports only live machinery
(via the generator, the removal helper, the log helper, and the options type),
so its dependency direction is dead-to-live, and it needs the sizing fallback
(cluster 9) — a function exported from a live module solely to keep this dead
consumer compiling.
*Role:* the replaced flow's lifecycle owner; the case's whole-file,
class-bearing variant of dormancy, plus a module-scope singleton reachable
only from dead code.

### 9. Viewport sizing fallback and resolver (`src/utils/generatePrintWindow.ts` + new file `src/utils/printPreviewViewport.ts`)

`applyLegacyViewportFallback` is appended to the live window-generating
module: given a reference element it sizes the window from its bounding box
(the early-draft alternative to the shipped `printIframeProps` sizing). The
new sibling module holds an exported `PrintViewport` interface, an exported
minimum-viewport constant, a private pixel-length parser, and an exported
resolver that clamps a reference element's box to the minimum and logs at
debug level.

*Why this pairing:* the current line's real work is iframe sizing
(`printIframeProps` width/height), so a leftover draft implementation is the
most plausible species of dormant code here. Splitting it across a dead
module *and* a live-module export exercises two of the case's structural
varieties at once: a pure helper module nobody imports, and an export in a
hot live module whose only consumer is the dead manager of cluster 8.
*Role:* alternate sizing derivation subordinate to the dormant limb.

## Deliberate structural variation

The limb intentionally spreads across four distinct structural forms rather
than repeating one:

- **branch dormancy inside live functions** — five sites with the block
  placed where the old behavior genuinely forked (observability, print
  driving, content source, shadow mechanism, state transfer), each with a
  distinct guarded action (pure logging, early return with deferred cleanup,
  id lookup and return, looping side effects, single helper call);
- **whole-module dormancy** — two files: a stateful class-based owner with a
  singleton, and a stateless pure-function resolver with interface, constant
  and private helper; only the former has import references (from nothing),
  the latter is imported by nobody;
- **dead helper-in-live-module** — three sites, deliberately varied in
  visibility and reach: one private helper (clone), one exported helper
  (state copy), one exported helper whose sole consumer is a dead module
  (viewport fallback);
- **data-only dormancy** — two shared identity constants, consumed only by
  dormant branches and a dead module.

Two deliberately varied dependency chains cross files: the content-resolution
branch depends on a shared constant (branch-to-data), and the dead session
manager depends on a live-module export (module-to-module). The guarded
blocks share one guard idiom because they consult the same switch, while the
code they guard varies in shape (single statement, early return, nested
loops, helper delegation).

## What the design avoids

Every dormant site is justified by the modeled maintenance history rather
than by a marker, and nothing live references anything dormant: the two
orphan modules are the only importers of the dead export, and the dormant
branches are the only users of the two constants and of the two same-file
helpers. The live pipeline, the public entry exports, the option types, and
all documented messages and behaviors are untouched. The 287 added lines
carry their own explanatory comments in the repository's voice, consistent
with how the project documents behavior notes in situ.

Whether the changes collectively amount to one coherent smell, and how much
unrelated work the diff contains, are left as conclusions for assessment;
this record documents what was built, where, and why that site and shape
were selected.
