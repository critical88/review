# Injection design record

## Maintenance motivation

The pinned state of the package supports `prompt_toolkit >=2.0,<4.0`, but its
current development line is entirely prompt_toolkit 3 based. The migration from
the earlier prompt_toolkit 2 / questionary 1.x era was rolled out behind opaque
compatibility flags: instead of deleting the old behaviour outright, each
affected site got a module constant (defaulting to the old behaviour's
off-state) that a downstream consumer could — in theory — flip back. That
escape hatch was never documented, never wired to configuration, and never
written anywhere after its single import-time assignment. The scheduled
follow-up ("remove the flags once we are sure nobody needs them") never
happened, which is exactly how dormant compatibility layers survive in real
packages: each piece was a defensible, individually small decision at the time,
and the layer persists as a set of flags nothing can turn on plus everything
those flags hold alive.

## Development evolution being modeled

Each injected fragment reproduces a shape that genuinely existed in this
package's lineage:

- **Event-loop bootstrap.** `questionary.utils.activate_prompt_toolkit_async_mode()`
  really began as a prompt_toolkit 2 compatibility shim, and the historical code
  called the prompt_toolkit `eventloop.use_asyncio_event_loop()` API. The
  injected `use_prompt_toolkit_2_event_loop()` helper plus the guarded branch in
  the async asking path model the pre-3.x way of preparing the loop, kept as a
  fallback that the current version handling made unreachable.
- **Name changes.** The 1.x line introduced today's `DEFAULT_SELECTED_POINTER`
  constant; before that rename, list-style prompts read a plain `SELECTED_POINTER`
  constant with the same default glyph. The injected duplicate models the old
  name surviving next to the new one for one more release.
- **Style merging.** Before the current merged-default style handling, every
  question merged its own style list onto the default list with a plain
  two-list merge (`[DEFAULT, style]` style concatenation). The injected
  `merge_styles_1_x` plus the checkbox branch model that pre-2.0 construction.
- **Layout assembly.** The current single control window with search and
  validation rows was assembled in stages; the 1.x era built a two-window
  layout (question/answer window, control window) with no search or validation
  rows. The injected `create_legacy_inquirer_layout` models the earlier layout
  builder kept switchable.
- **Path completion.** The 1.x era completed paths by re-parenting the input and
  silently skipping directories that did not exist, instead of reporting a
  validation error. The injected `legacy_get_paths` re-binding models that
  behaviour kept behind a flag.
- **Deprecated prompt names.** `questionary` deliberately kept accepting the 1.x
  prompt names (`list`, `rawlist`, `input`) and resolving them through the
  supported runtime registry to today's implementations. The injected retired
  registry + adapter factories model the intermediate design in which those
  names were switched to hand-rolled 1.x re-implementations.
- **Registry dispatch.** The package resolves prompt names through a module
  registry dict today; a dormant second registry next to it is the natural
  place for a deprecated resolution path to hide, because a dict named in a
  package `__init__` looks like part of the module's public data.

## Overall design

The injected layer is spread over the whole production package rather than
concentrated in one module, mirroring how a compatibility layer actually grows:
one flag per era-sensitive site, placed in the module that owns the behaviour it
guards (a style flag next to the style module, a layout flag next to the layout
module, a dispatch flag next to the dispatch code). Every flag is an inert
module-level constant — falsy literal, exactly one import-time write, no other
write anywhere — so each guarded branch is statically unreachable in every
supported configuration. The branches themselves were placed at the earliest
point where the old and new behaviour diverge (before await, before style merge,
before layout construction, at completer initialization, before name resolution),
which is where a real migration would have put its switch.

Around the branches, the layer keeps the full auxiliary structure a maintained
fallback needs: an exclusive helper per branch (event-loop bootstrap, legacy
layout builder, legacy style merge, legacy directory resolution), and, for the
dispatch site, a registry of deprecated names mapping to adapter factories that
re-implement the 1.x question behaviour and, in turn, read a duplicated legacy
pointer constant. The reference chains therefore run more than one level deep:
flag -> branch -> registry -> factory -> constant. That depth is what makes a
syntactic sweep insufficient and forces a repair to follow actual references.
The design deliberately leaves the supported routes untouched — the live
runtime-writable async activation flag, the prompt_toolkit version handling, the
supported registry that resolves the backwards-compatible names to today's
factories, and all current defaults — so a correct removal is a pure deletion
with no behavioural consequence.

## Per-cluster rationale

### Async entry point — `questionary/question.py`, `questionary/utils.py`

The async asking path is the package's most load-bearing entry, and its
`Question` class is the natural place for a "prepare the loop the old way"
branch: the event-loop decision happens right before awaiting. The switch lives
in the utilities module beside the real async activation flag, exactly where
this package keeps event-loop policy; the bootstrap helper sits next to the
current activation flow it once served. A `utils.SWITCH` (module attribute) was
chosen over a local variable so the branch test resolves through a real
cross-module reference — the kind an eager maintainer must actually trace.

### Checkbox style assembly — `questionary/styles.py`, `questionary/prompts/checkbox.py`

The checkbox question is the one factory that still does bespoke style assembly,
so branching on "merge the old way" there is a realistic per-question-era
switch site. The old merge helper is hosted in the style module next to the
current merged-default handling, and the style flag sits with it — policy next
to the domain it names.

### Shared layout assembly — `questionary/prompts/common.py`

`create_inquirer_layout` is shared by every list-style question, which makes it
the highest-traffic production role in this slice: an old-layout branch there is
the most expensive place a misunderstanding could bite. The legacy builder is a
sibling function in the same module (where a real two-era layout module would
keep both builders), and the layout flag is a local constant of that module —
a second place, other than `utils`, for a maintainer to look for switch state.

### Path completion — `questionary/prompts/path.py`

The completer's constructor is where directory resolution is wired, and is the
only class-body site in the layer, so the dormant branch takes a
method-rebinding shape (`get_paths` re-bound to the legacy wrapper) rather than
a return branch — the shape a monkey-patch-style fallback would really take.
The legacy wrapper and its flag are module-local to the path module.

### Deprecated-name dispatch — `questionary/prompts/__init__.py`, factories, `questionary/constants.py`

The name-resolution function is where the supported registry is already read,
so a second, retired registry plus a guarded lookup models the superseded design
most faithfully. Hosting the retired registry and its flag directly in the
package `__init__` makes the dispatch cluster package-level data rather than a
function-local detail, and its factories are hosted in the per-question modules
they re-implement. The three adapters read the duplicated legacy pointer
constant from the shared constants module — the deepest edge in the chain, and
the one place the layer touches shared data rather than executable code.
