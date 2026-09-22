# Injection design record — vis, viewport scroll progress features

## Maintenance motivation

Three requests keep recurring for a terminal editor like vis:

1. **A vim-style indicator in the status bar.** Vis already prints a
   percentage there, but users coming from vim expect the informational
   forms `All`, `Top` and `Bot` as well, so that the focused window states
   its position in words rather than only digits.
2. **A scrollbar.** In an editor whose windows are as short as a terminal
   and whose files are not, a right-edge scrollbar communicates how much
   of the file is above and below the viewport at a glance — the reason
   nano, kakoune and vim's own plugins all carry one. The UI rewrite that
   gave every window a per-cell buffer (arbitrary glyphs per cell, not just
   characters) removed the last technical obstacle: the rightmost column of
   a window is now blank real estate that can carry drawing.
3. **Scroll state for plugins and motions.** Lua plugins format their own
   statuslines and need the same numbers, and power users want to
   yank/change "what is on this screen row" the same way they already use
   the paragraph or line text objects.

A maintainer picking these up would land them as one "window scroll
reporting" overhaul: the requests touch the same data (where the viewport
sits inside the file), the same window object, and would naturally ride the
same release.

## Development evolution being modeled

The visible step is easiest to describe as: the editor already *has* the
knowledge, and the features only *read* it. `View` (view.h) maintains the
linked list of screen lines with their byte lengths and file line numbers,
the viewport byte range `view->start .. view->end`, the display height, and
the primary selection. Every new consumer holds a `Win*`, a `Vis*`, or a
`lua_State` from which `win->view` is one field away — `Win` embeds its
`View` directly. The path of least resistance in this codebase, and what a
pragmatic patch usually does, is to read those fields directly at the point
of use: the pieces of the walk needed for each feature already exist in
`view.c` and `window_status_update` demonstrates the percentage arithmetic
that every one of these requests wants.

The change under review models the release where that overhaul landed that
way: four consumers, each digging for the same geometry from the object it
already holds, and each extending the module it lives in — status
composition in the UI layer, glyphs in the paint loop, a method in the
Lua window glue, an entry in the text-object table.

## Overall design

The release adds "where are we scrolled to" as a user-visible concern in
four places, each following the conventions of its host module:

- `view.c`'s status update previously computed a bare percentage itself; it
  now asks the UI layer to compose the full right-most segment, the way the
  window's `ui_window_*` helpers already own other aspects of presentation.
- The terminal backend gets the indicator composer and a scrollbar painter,
  both hanging off the existing `ui_window_draw` pass, so drawing stays a
  single sweep over the window's cells.
- The Lua window object gets a `scrollinfo` method returning a small table,
  consistent with the existing `window_index`/`window_status` bindings.
- A new text object selects the currently displayed screen row, joining the
  existing semantic units (paragraph, line, brackets) in the dispatch table
  with the standard three-argument text-object signature.

Two header lines support these: a declaration for the new UI helper in
`ui.h` (the other `ui_window_*` declarations live there) and a text-object
id in `vis.h` ahead of the terminating marker, as the dispatch table is
indexed by that enum.

## Per-cluster rationale

### Status indicator segment — `view.c`, `ui-terminal.c`, `ui.h`

`window_status_update` (view.c) fills the left and right segment arrays of
the status bar. Its previous inline percentage block is replaced by a call
to the new `ui_window_indicator`, which composes the whole segment. The
composer lives next to `ui_window_status` and the other `ui_window_*`
helpers, since in this codebase the UI module owns the running status bar
string and knows how segments are joined with ` » `/` « ` separators, while
the classification (`All`/`Top`/`Bot`/`NN%`) mirrors what vim users expect
to read. The function walks the window's line list to count the used
screen rows, mirrors the cursor's screen row against the bottom line and
the primary selection, and derives the percentage from the viewport window
relative to the full text size — the same arithmetic the old inline block
used, plus the booleans deciding whether the file start or end is
currently in view. The call site keeps the status logic assembled in one
function: the segment arrives as a string exactly like the pending-keys and
`line,col` segments beside it.

**Production role:** the right-most status segment of the focused window,
updated on every status refresh, i.e. the primary "where am I" readout of
the editor.

### Right-edge scrollbar — `ui-terminal.c`

`ui_window_scrollbar` paints, at the end of `ui_window_draw`, the unused
last column of the window: a track of `│` glyphs bounded by the number of
used rows, a `┃` thumb spanning the visible fraction, and a `▐` notch on
the cursor's screen row, drawn through the bounds-checked
`ui_draw_string`/`ui->cell_buffer` machinery with the existing
`UI_STYLE_*` ids. Because cells already hold the file content, the
scrollbar writes over the drawing area's spare column the same frame the
rest of the window is painted, keeping window drawing one pass.
The thumb interval is computed by scaling the viewport byte window to the
viewport height, and the track length by counting the used rows, so the
glyphs are placed from the same line list that content drawing uses.

**Production role:** presentation — every window with content shows how
its viewport slides over the file, focused or not, on the curses build.

### `win:scrollinfo()` scripting access — `vis-lua.c`

The window method table (`window_funcs`) gains `scrollinfo` next to
`status`. The callback lives with the other window bindings, type-checks
its argument like they do, and returns a table of six integer fields:
top-line number, last visible line number, number of used screen rows,
viewport height, row of the cursor inside the viewport and the percentage
— with the last-line/booleans available for statusline plugins. Following
the table-driven style of `window_index`, the function reads the state it
needs from the window's view directly and pushes plain Lua integers, so
plugin authors get a stable, allocation-free accessor modeled after the
API this tree already exposes to lua.

**Production role:** the scripting surface — what plugins call instead of
scraping the status string, part of the stable per-window API.

### Screen-row text object — `vis.h`, `vis-text-objects.c`

`VIS_TEXTOBJECT_SCREENLINE` was added to the text-object idenum, and the
dispatch table in `vis-text-objects.c` maps it to
`vis_text_object_screenline`, using the standard three-argument signature
the table requires (`vis`, `txt`, `pos`). The function locates the row
containing `pos` by walking the screen lines from the top of the
viewport, accumulating each row's byte length, and — because the text
layer's notion of "line" can span several screen rows under line
wrapping — extends the range over consecutive screen rows that share the
same file line number, bounded by the viewport's end.

**Production role:** a semantic editing unit registered in the built-in
text-object table next to the paragraph and bracket objects, usable by
operators and custom mappings through its text-object id (the tree wires
no default key for new objects), independent of how the file's physical
lines wrap onto the screen.

## Choices worth recording

- All walks happen at the consumer because every consumer already holds
  the window object, and `win->view` is embedded, public and fully
  documented in `view.h`. No struct was opened up or widened; nothing was
  added to any header's data layout. The diff therefore contains only
  function/declaration/table additions and the one call-site change in the
  status code.
- The multi-line UTF-8 scrollbar glyphs were chosen because cells hold
  bytes, not characters; any glyph fits the existing width bookkeeping.
  In headless teletype-style captures they may render as blanks, just as
  the tree's own existing multibyte display symbols do.
- The user-visible semantics were pinned to the well-known vim/nano
  behavior (All/Top/Bot forms, inclusive 1-based percentage bounded by
  100, scrollbar collapsing to a single-row thumb at the extremes) rather
  than inventing new conventions, which is what keeps the resulting editor
  behavior natural to review.
