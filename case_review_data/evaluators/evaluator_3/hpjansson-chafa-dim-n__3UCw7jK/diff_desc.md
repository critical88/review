# Injection design record — chafa deep-inlining (hpjansson-chafa-dim-n)

## Maintenance motivation

Chafa's symbol-mode rendering pipeline is a chain of production phases: build
the symbol map (`chafa_symbol_map_prepare`), generate the palette
(`chafa_palette_generate`), rasterize the character grid (the symbol renderer's
per-row cell update), and emit terminal sequences (`chafa_canvas_print_symbols`).
Each phase has one driver function that delegates to a layered call tree of
static helpers. Performance work on this pipeline routinely tempts a developer
to "avoid call overhead" or "keep the fast path in one place": the per-cell
selection loop runs tens of thousands of times per frame, palette construction
walks every pixel, and emission formatting runs per character run. The natural
result is that a maintainer copies the bodies of the callee layers up into the
driver and deletes the now-unused helpers, "temporarily", with a comment such
as keeping the hot path together. The diff models exactly this development
step: an aggressive manual flattening of the driver call trees of all four
phases of one pipeline, performed while optimizing the symbol-mode path.

## Evolution being modeled

The normal repository evolution this mirrors is a hand-performed inlining
campaign: helper functions disappear one by one, their implementations are
adapted into the caller at the point of the former call, local variables of
the callees merge into the caller's scope, and early returns become goto-style
labels or inlined conditionals. Such rewrites appear in real performance
work; the distinguishing property modeled here is the removal of the
decomposition itself — the caller absorbs three or more distinct former call
levels — rather than a rename or a relocation of code between files.

## Overall design

Four driver functions, one per lifecycle phase of the same pipeline, each
absorb the full implementation chain they used to delegate to:

| Phase | Driver | Former helper levels whose bodies it absorbs |
|---|---|---|
| Map build/prepare | `chafa_symbol_map_prepare` (chafa/chafa-symbol-map.c) | map-preparation sub-branches, per-format symbol-file processing, comment stripping, glyph bitmap conversion and table compilation |
| Palette quantization | `chafa_palette_generate` (chafa/internal/chafa-palette.c) | type-dispatched generation strategies, pixel bin sampling, PNN cluster merging, transparent-pen cleanup |
| Cell rasterization/selection | `update_cells_row` (chafa/internal/chafa-symbol-renderer.c) | per-cell update, slow/fast symbol candidate picking, candidate evaluation, mean-color extraction, plain error scoring, per-cell color quantization, fill fallback |
| Output emission | `chafa_canvas_print_symbols` (chafa/internal/chafa-canvas-printer.c) | ANSI string building, per-row formatting, per-color-mode emission, SGR attribute handling with pen reuse |

Each rewrite keeps the driver's public signature, its position in the
pipeline, and the exact sequences it emits; the pipeline's data flow and
external behavior are preserved by construction. Only single-caller helpers
are deleted; functions that also serve the untouched sibling paths (wide-symbol
cells, symbol-rows emission, symbol map apply) remain in place, which is what
a careful but misguided maintainer would produce.

The four clusters intentionally use different adaptation idioms so the
flattened code does not read as one mechanical pattern:

- The symbol-map driver keeps C statement order and table-walk structure, with
  multi-line parsing conditionals folded in at their former call sites.
- The palette driver inlines data-driven branch bodies behind a small
  dispatch, merging loop nests that the helpers previously separated.
- The cell-selection driver converts early returns into label-guarded
  sections, introduces accumulator loops in place of helper calls, and merges
  the slow/fast strategy branch bodies into one branch tree.
- The emission driver converts a mode dispatch on an enum into an if / else
  chain, flips ternary threshold picks into if/else statements, and unrolls
  a two-branch symmetric call site into a single inlined site with swapped
  pen variables.

## Per-cluster explanations

### chafa/chafa-symbol-map.c — `chafa_symbol_map_prepare`

This site was selected because symbol map preparation is the pipeline's
upstream phase: a self-contained driver whose helper tree processes symbol
selectors, compiles glyph bitmaps, and builds the sorted symbol tables used
by every later phase. The former sources are heterogeneous — user-facing
selector parsing, bitmap conversion, and table sorting — so the inlined body
mixes loop nests, parsing conditionals and bitmap walks, which is what makes
the collapse difficult to unwind mechanically. The copied blocks adapt
helper arguments into the driver's local variables and keep each block in its
original position relative to the surrounding statements, so the production
role (all three map variants: built-in, user-specified, and custom glyph
imports) stays recognizable in the collapsed body.

### chafa/internal/chafa-palette.c — `chafa_palette_generate`

The palette driver was chosen as the pipeline's quantization phase: its
strategy dispatch (basic/fixed/dynamic palettes) and the PNN quantization
walk cover computation-heavy code with numeric invariants (bin sizes, cluster
distances, iteration counts). Only the dynamic-pixel collection and strategy
entry helpers joined the driver; the nearest-color lookup helpers remain
separate because the per-cell phases still call them. This keeps the cluster
representative of a numeric pipeline stage rather than duplicating the
selection-phase role.

### chafa/internal/chafa-symbol-renderer.c — `update_cells_row`

The per-row cell update is the pipeline's hot inner loop and the file whose
sub-tree is the deepest: cell update, symbol candidate selection (slow and
fast variants), candidate evaluation, work-cell mean color extraction, plain
error scoring, 16/8 color quantization, and fill fallback. The rewritten
driver keeps the single-caller helpers' early-return semantics by hoisting
the guard conditions into the surrounding branches and reusing one label for
the fill-fallback tail. The wide-symbol family (`update_cells_wide` and its
helpers) is untouched: it serves a separate symbol family and shares several
of the smaller helpers, so it both exercises the original decomposition and
bounds how far the rewrite can be simplified away by deleting call sites.

### chafa/internal/chafa-canvas-printer.c — `chafa_canvas_print_symbols`

The emission phase converts the internal formatted string tree into the
public print entry point, so it covers terminal-state handling (SGR attribute
tracking, pen reuse, inverted video) rather than numeric computation. The
former `chafa_canvas_print_symbols` was a two-line delegation to
`build_ansi_gstring`; after the rewrite the entire emit pipeline — row
buffering, per-mode color emission, attribute state machine, character run
queueing — executes in its body. The rows-oriented sibling entry points and
the small emitter primitives (character queue flushing, pen updates) stay as
shared functions for the same reason as in the renderer: they still serve
untouched paths, and a realistic inlining campaign stops at multi-caller
helpers.

## Behavior and compatibility boundary

All four drivers keep their exact signatures and positions in the pipeline.
The rewrites preserve the emitted terminal sequences, the palette and symbol
map contents, and the rendered cells for every mode the pipeline supports
(color spaces, symbol selections, fill and invert handling, thread counts,
grid layouts and work factors), and every helper that other translation
units or sibling paths still consume remains in place with unchanged
signature.
