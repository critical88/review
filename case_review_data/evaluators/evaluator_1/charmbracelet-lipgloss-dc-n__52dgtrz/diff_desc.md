# Injection design record — table and tree rendering configuration (task `charmbracelet-lipgloss-dc-n`)

This record documents the design of the injection for the assigned smell category
(data clumps) in lipgloss at commit
`6a419c6543d3475a369ef08f6252a2a6f33be733`. It describes the maintenance
motivation, the ordinary development evolution being modeled, the overall design
of the change, and the rationale for each changed location. It is an auditable
design record for the produced diff, not a solver guide.

## Maintenance motivation

Both of lipgloss's composite renderers — `Table` and `Tree` — hide their
rendering configuration inside object state:

- In `table`, the construction helpers (`constructTopBorder`,
  `constructBottomBorder`, `constructHeaders`, `constructRow`,
  `computeHeight`, `truncateCell`) are all methods on `*Table` that read
  `t.border`, `t.borderStyle`, the seven `border*` booleans, `t.widths`,
  `t.heights`, `t.headers`, `t.wrap` and friends implicitly. `Table.resize()`
  additionally snapshots Table's border flags, wrap policy, y-offset and
  manual-height settings into the `resizer` struct it builds, so those policy
  values live in two places at once.
- In `tree`, per-tree rendering configuration lives on a separate `renderer`
  object (`style`, `enumerator`, `indenter`, `width`) that each `Tree`
  materializes lazily behind a `sync.Once` (`ensureRenderer`), and the render
  helpers are methods on that object.

The motivating complaint a maintainer would voice about this code is a classic
one: the helpers are *implicit* about what they consume. You cannot tell what
`constructRow` depends on without reading its whole body through the receiver,
the resizer duplicates the table's policy fields, and the tree's renderer
wrapper forces every per-subtree styling variation to go through a lazily
constructed object graph. The natural refactor a developer reaches for in that
situation is to make every helper spell out its inputs explicitly, and to stop
copying policy into the resizer.

That refactor is genuinely useful work, and it is exactly the evolution modeled
here: pass the values each helper needs as explicit parameters instead of
implicit object state. Performed on many related helpers at once, its visible
side effect is that the same cohesive groups of values — the table's border
context, the horizontal layout policy, the tree's enumeration/render
configuration — now travel as loose parameter lists from helper to helper
instead of remaining with one owner. This design intentionally leaves them
threaded that way; bundling them behind a deliberate abstraction is a separate
design step that the injected code does not take.

## Normal development evolution being modeled

The diff models the state a package drifts into after incremental work of this
kind, not a sudden rewrite:

1. A developer wants the table's frame builders to be callable without a
   `Table` (to test border composition, or to reuse it for ad-hoc frames), so
   the two pure frame builders are pulled out into free functions and given
   explicit inputs. Their parameter lists repeat because every frame needs the
   same context.
2. The same treatment is applied to the builders that must stay methods because
   they still need `t.style` and `t.data`: their remaining inputs are made
   explicit, so their parameter lists repeat the frame context again.
3. The resizer is asked to stop snapshotting Table's policy into its own
   fields; instead the sizing entry point passes the layout policy down through
   each sizing pass. The resizer keeps only the geometry it computes itself
   (column statistics, row heights, paddings).
4. In `tree`, the `renderer` wrapper is dissolved: each `Tree` holds its
   rendering configuration directly, and the render helpers become free
   functions that receive the configuration as explicit arguments so they can
   render any configuration, not just the one captured on a wrapper object.

Each step is the kind of change a codebase accumulates while someone is "just
making the dependencies visible" or "removing an unnecessary wrapper". The
cohesive groups of values that the old objects held together are, at the end,
handed around as individual arguments through every signature in the affected
pipeline.

## Overall design

- Behavior-preserving by construction: every helper performs the same
  computation on the same values it used to read implicitly. Rendered output is
  unchanged for every table shape (borders, fitted and fixed widths, wrapping,
  truncation, multiline cells, manual heights, y-offset scrolling with the
  overflow/ellipsis row) and every tree shape (default and custom enumerators
  and indenters, per-item styling, width padding, hidden subtrees, per-subtree
  configuration override).
- The change spans the four production files that participate in these two
  pipelines: `table/table.go`, `table/resizing.go`, `tree/renderer.go`,
  `tree/tree.go`. No test files are modified; no exported API changes.
- The four files are covered as one coherent task ("make the rendering helpers
  explicit about their inputs") applied consistently across both rendering
  subsystems, rather than as unrelated edits.
- Cohesive sets of configuration values are deliberately not given a shared
  carrier in the injected state: the table's border context, its horizontal
  layout policy and the tree's enumeration/render configuration are all threaded
  member-by-member, so the repetition is visible at each call site.

## Per-cluster rationale

### 1. Frame and grid builders — `table/table.go`

**What changed.** `constructTopBorder` and `constructBottomBorder` are now free
functions taking `border`, `borderStyle`, `borderLeft`, `borderRight`,
`borderColumn` and `widths`. `constructHeaders` and `constructRow` remain
methods (they still need `t.style` and `t.data`) but now take the border
context, headers/widths/heights, wrap policy and overflow height explicitly.
`computeHeight` takes `heights`, `hasHeaders` and the four vertical border flags.
`truncateCell` takes the cell's height inputs instead of reading `t.heights` and
`t.headers`. `String()` hands its fields to each helper member-by-member.

**Why this location and shape.** These helpers are the whole frame of the
rendered table; they are the sites where the table's configuration is consumed
most intensively, and they are genuinely reusable in isolation — a border
builder needs nothing but the border context and column widths. The two pure
builders became free functions because they need no receiver at all; the header
and row builders kept their receiver because cell styling and data access stay
on `Table`. `String()` is the assembly point, so it is the natural place from
which to thread the configuration outward.

**Production role.** Top/bottom border rendering; header row rendering with its
separator; data row rendering; overflow row rendering; total-height
computation; cell truncation.

### 2. Sizing pipeline — `table/resizing.go`

**What changed.** The `resizer` no longer stores the border flags, wrap
policy, y-offset or manual-height setting; it keeps only the geometry it
computes (column statistics, row heights, y-padding matrix). `optimizedWidths`,
`expandTableWidth` and `shrinkTableWidth` now take `borderLeft`,
`borderRight`, `borderColumn` and `wrap` per call; `expandRowHeights` takes
`wrap`; `detectTableWidth` and `totalHorizontalBorder` take the three separator
flags; `visibleRowIndexes` takes the four vertical border flags together with
`yOffset` and `useManualHeight`. `resize()` supplies these values from `Table`
at each call.

**Why this location and shape.** The resizer/Table boundary is where policy
values previously crossed by being copied into another struct — the clearest
encapsulation defect in the pipeline, and a plausible first target for someone
who wants explicit dependencies. Passing policy per call removes the duplicated
fields, and since every sizing pass needs the same separator and wrap decisions
to compute a correct total width, the same small set of values is repeated
through the sizing family — including the row builder on the other side of the
package boundary, which consumes the same separators and wrap policy when it
lays out cells.

**Production role.** Width detection and fit-content behavior; the expand pass;
the shrink passes (largest-first and median-based, with and without the column
width floor); row-height expansion under wrapping; visible-range and overflow
computation for manual heights.

### 3. Tree renderer dissolution — `tree/renderer.go` and `tree/tree.go`

**What changed.** The `renderer` wrapper is dissolved. `Tree` now stores the
rendering fields (`style`, `enumerator`, `indenter`, `width`) with a
`renderSet` marker, materialized lazily by `ensureRenderConfig()` behind the
existing `sync.Once` so an unconfigured tree still renders with the same
defaults. `enumPrefix`, `maxEnumPrefix`, `renderTree`, `renderChildren` and
`renderChild` are now free functions over explicit parameters:
`renderTree` receives the whole render configuration (enumerator, indenter,
root style, the three style functions, width) plus `root` and `prefix`;
`renderChildren` and `renderChild` additionally receive the enumeration context
(`children`, `enumerator`, the enumerator style function) and the computed
`maxLen` prefix-alignment width. The per-child override now checks the child's
`renderSet` marker instead of a wrapper pointer, and `Tree.String()` expands its
fields into arguments to start the recursion.

**Why this location and shape.** The tree pipeline is the second natural home
for the same design step, and it takes a different shape from the table one:
here the configuration travels *through a recursion*, and one of the threaded
values (`maxLen`, the widest enumerator prefix among siblings) is a value
computed along the way rather than stored config, which is why the prefix pass
was factored into `maxEnumPrefix` (`enumPrefix` renders one prefix,
`maxEnumPrefix` prunes hidden trailing children and measures the widest
prefix). Keeping the helpers as free functions is what lets the child-override
switch substitute a descendant subtree's configuration member-by-member into
the next recursion step. `tree.go`'s setters each call `ensureRenderConfig()`
before writing, exactly as they previously called `ensureRenderer()`, so an
unset style never overwrites materialized defaults with zero values.

**Production role.** Root rendering with width padding; enumerator prefix
rendering and sibling alignment; indenter-based recursion with multiline
prefix handling; per-subtree configuration override; lazy default
materialization for unconfigured trees.

## What was intentionally left alone

Regions explored and excluded from the change are recorded in
`case_plan.json` (`structural_reach.excluded_candidates`). In short: the core
lipgloss utilities (join/align/wrap/color/position) take per-call geometry, not
cohesive configuration; the table data layer and tree child-maintenance helpers
have no configuration flow; and extending the vertical border flag quartet
beyond its two genuine consumers, or threading per-column widths into
single-column truncation helpers, would have added parameters to functions that
use only fragments of the set — padding rather than a believable design step.
