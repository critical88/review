# Injection design record: line-number gutter maintenance (kibi @ 5b8715f)

## Maintenance motivation

The line-number gutter is one of the few features in kibi that crosses the
whole rendering pipeline. Before this change, everything about it lived in the
editor: `update_screen_cols` open-coded the digit-count arithmetic with
`successors`, decided in the same breath whether the gutter fits the window,
and derived both `ln_pad` and `screen_cols`; `draw_left_padding` formatted the
right-aligned number and the bar inline, with the dark grey color written as a
magic string next to it.

That conflation is the kind of coupling reviewers routinely ask to break
apart: the editor method was doing arithmetic, policy, and presentation at
once, while each of those pieces has a module in the crate that specializes
that concern. This change models the normal refactor that follows such a review:
introduce a small value type describing the gutter's width where the row data
model lives, and distribute the pieces of gutter decision-making to the
modules that are supposed to own each kind of decision.

## Normal development evolution being modeled

The evolution is the ordinary one a feature undergoes when a data-model type
gains maturity and each consuming module starts asking for its own slice of
behavior around it:

1. A value object (`GutterWidth`) is introduced on the row data model, with
   two purely structural operations: constructing it from a row count (the
   old `successors` digit counting, moved verbatim) and reading its total
   padding (digit count plus the two columns used by the space and the bar).
2. The configuration module, which owns the `show_line_numbers` preference,
   gains the policy predicate that combines the preference with the width
   value and the window size. As a pure function of its three inputs, it is
   `const` and fits the module's existing style of small parsing/derivation
   helpers.
3. The terminal module, which owns window-size concerns, gains the
   column-budget calculation: given the window width and an optional gutter
   width, how many columns remain for text.
4. The escape-sequence module, which owns every other drawing primitive
   (`push_colored`, `RESET`, `WBG`, ...), gains the gutter entry formatter and
   the color constant for the bar, next to the other sequences it defines.
5. The editor stops computing any of this itself: it asks the new helpers in
   sequence, and gains one accessor that adapts editor state (row count,
   configuration, window width) into the gutter value plus the visibility
   decision, used on the drawing path.

Nothing in the sequence is exotic; each step is the kind of pull request a
maintainer writes to "move logic closer to its owner". The record of interest
for this task is the cumulative position the code ends up in: a concept with a
named type in one module, and decision-making about that concept spread over
signatures in several others (see the design section).

## Overall design

The change keeps the rendering contract exactly as before — the same
`successors` digit counting, the same `n_digits + 2` padding, the same
`show_line_numbers &&` quarter-window visibility rule, the same
`window_width - ln_pad` text budget, and the same
`format!("{:>1$} \u{2502}")` entry with `\x1b[38;5;240m` — but re-expresses
each fact once, in a module that specializes it, with the width always
traveling as the `GutterWidth` value type rather than as a bare `usize`.

- `src/row.rs` defines `GutterWidth` (`n_digits: usize`, `Clone + Copy`),
  `GutterWidth::from_row_count` (digit counting via `successors`, moved from
  the editor) and `GutterWidth::padding` (`n_digits + 2`).
- `src/config.rs` gains `gutter_shown(show_line_num, gutter_width,
  window_width) -> bool`, a `const fn` policy next to the other small helpers
  in that module.
- `src/terminal.rs` gains `text_area_cols(window_width, Option<GutterWidth>) ->
  usize`, the text-column budget.
- `src/ansi_escape.rs` gains `GUTTER_COLOR` and
  `gutter_entry(gutter, val) -> String` (right-aligned `val` within
  `padding - 2`, then the space and bar), next to `push_colored`.
- `src/editor.rs` rewrites `update_screen_cols` to call the two free functions,
  adds `Editor::gutter` adapting editor state, and rewrites
  `draw_left_padding` to use the accessor plus the escape-sequence
  formatting helper. The vertical-scroll and row-drawing paths otherwise stay
  untouched.

The new items are `pub(crate)`; no public API, dependency, or configuration
surface changes.

## Per-cluster rationale

### `src/row.rs` — concept and construction (home of the data model)

The row module already owns everything derived from row content and row
layout, and it is where the digit-count input (the number of rows) is most
meaningful, so the value type is defined here. Only the two structural
operations are kept: `from_row_count` (the digit counting, moved verbatim from
the editor) and `padding`. The production role of this cluster is to be the
vocabulary: every other module speaks about the gutter through this type. The
implementation shape — a two-method `Copy` struct — is deliberately minimal so
that the type reads as a value, not as a behavior home. The struct field is
private, so the type cannot be built or dissected outside the module by
accident.

### `src/config.rs` — policy fragment

The visibility rule reads a user preference (`show_line_numbers`) that this
module parses and owns, so the predicate landed here as a `const fn` taking
the three inputs it needs. Its production role is the governance fact of the
feature ("given the preference and the current window, is the gutter shown at
all?"). The signature intentionally takes the `GutterWidth` value rather than
a raw digit count: the policy should not re-derive width facts, only combine
them. The module is otherwise test-heavy (the INI parser tests follow it), so
the new function sits above the test module with its own doc comment
explaining the quarter-window reasoning, which is the rationale the original
editor comment carried.

### `src/terminal.rs` — budget fragment

This module answers terminal-geometry questions (window size, cursor
position, terminal lifetime). "How many columns remain for text once the
terminal real estate is divided" fits that charter, so `text_area_cols`
landed here. The `Option<GutterWidth>` parameter models "no gutter shown"
explicitly instead of smuggling `0` — the production role is the single
place where the subtraction `window_width - padding` is written. The
implementation keeps the original `saturating_sub` so narrow windows behave
as before.

### `src/ansi_escape.rs` — presentation fragment

Every other drawing primitive lives in this module, and `draw_left_padding`
was calling the magic color string inline; moving both the constant
(`GUTTER_COLOR`) and the entry formatter here puts them with their natural
siblings. The signature takes the `GutterWidth` value plus a generic `val`
(line number, or the `~` marker for the trailing rows), and the body is the
verbatim `format!` from the editor. The production role is the formatting
fact of the feature; the generic mirrors `Editor::draw_left_padding`'s own
generic so no call site loses type information.

### `src/editor.rs` — orchestration fragment

The editor holds all the state the gutter depends on (row count, window
width, configuration), so it keeps an accessor, `Editor::gutter`, that adapts
that state into the value plus the visibility decision, re-using the policy
from the configuration module rather than repeating the condition — the same
condition is therefore reachable from both the refresh path
(`update_screen_cols`) and the drawing path (`draw_left_padding`). The
production role is adaptation and sequencing: `update_screen_cols` now
composes `GutterWidth::from_row_count`, `config::gutter_shown` and
`terminal::text_area_cols`, and `draw_left_padding` composes the accessor
with `ansi_escape::gutter_entry`. Keeping the accessor on `Editor` (rather
than a free function taking four arguments) preserves the module's style of
surfacing state through small methods.

A structural side effect of this split is that the editor ends up holding the
same gutter facts in two shapes. The refresh path composes the three helpers
directly and commits their results to `self.ln_pad` / `self.screen_cols`,
while the drawing path re-derives the width and the visibility decision live
through the accessor instead of reading the committed fields. Both paths
evaluate the identical inputs (preference, row count, window width), so every
render is internally consistent, but the stored state and its recomputation
now live side by side: the editor adapts editor state in two places that a
future change to the gate arithmetic has to treat together, and the fields the
editor relies on elsewhere (`screen_cols`) are written by one path while the
other path re-answers the same question from scratch each draw. This is the
position the code settles into when each module owns its own slice of the
decision and no single place assembles the answer once; it matches how the
affected rows are painted in practice — every draw goes through the accessor,
while the budget survives as committed state for everything else the editor
does with `screen_cols`.

## Notes on scope

The change touches only the five `src` modules described above and leaves
tests and build files as they were. `Editor::draw_left_padding` keeps its generic and its
callers' expectations; `update_screen_cols` keeps its `ln_pad`/`screen_cols`
field writes, so `draw_rows`, the scrolling code and the welcome screen see
the same values as before. The `successors` import moves with the digit
counting it belongs to (`row.rs` uses it; the editor no longer needs it).
