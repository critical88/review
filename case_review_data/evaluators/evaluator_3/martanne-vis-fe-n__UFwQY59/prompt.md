# vis: give the window scroll-progress features a proper home

## Background

vis is a modal editor for the terminal. In the last release it picked up a
batch of "where is the window scrolled to" features in one push:

- the focused window's status bar ends in a position indicator: the literal
  text `All`, `Top`, `Bot`, or a percentage rendered as `NN%`;
- while a window is drawn, a scrollbar appears in its last screen column:
  a track bounded by the used rows, a thumb spanning the visible slice, and
  a highlight at the cursor's row;
- Lua scripts can query a window's scroll state through the
  `win:scrollinfo()` method, which returns a table with the integer fields
  `topline`, `lastline`, `lines`, `height`, `row`, `percent`;
- a text object covering the currently displayed screen row the cursor is
  on was registered alongside the built-in units (when a file line wraps
  over several screen rows, the range extends over its remaining rows).

## The problem

Working in this feature area has become unpleasant. Everything above reports
the same few facts - which byte range the viewport shows, how those bytes
are laid out into screen rows, and where the cursor sits among them - but
each feature re-derives them on the spot, wherever the code already happens
to hold a window. Symptoms we have run into:

- a change to how the cursor's screen row is determined had to be repeated
  in more than one place, and nothing in the code structure tells you which
  places;
- reviewing any of these features means mentally replaying a walk over
  screen lines and the primary selection to see whether two versions of it
  compute the same thing;
- knowledge about the on-screen layout, which the editor already maintains
  continuously while drawing, is being rebuilt in code whose actual job is
  painting, formatting a status string, handing values to a script VM, or
  answering "what text does this unit cover";
- when a piece of that duplicated logic is no longer needed, it is unclear
  which copies can go and which are still holding up a wall.

## What we would like

Please treat this as a code-placement problem, not a feature redesign:

- Figure out which part of the codebase is the natural owner of "what the
  viewport is currently showing" and of the facts derived from it (visible
  slice, used rows, cursor row, progress through the file), and let that
  owner compute them.
- Have each of the four features above obtain those facts from the owner
  instead of re-deriving them. Feature code stays responsible for
  presentation, string formatting, marshalling values to scripts, or
  semantic selection respectively - not for the derivation itself.
- Dissolve the duplicated logic once this is done: retire walks, helpers,
  and declarations that no longer have a reason to exist, so the same
  knowledge is not left sitting in several layers. The end state should
  read like the codebase would have looked had these features been built
  that way from the start.

## Behavior that must not change

User-visible features and any interface used by scripts or plugins keep
working the same way:

- the status bar's right-most segment for the focused window still shows
  `All` when the whole file fits, `Top`/`Bot` at the respective ends
  (with `Top` also requiring the cursor on the first screen row), and
  otherwise a percentage from 1 to 100 followed by `%`, reflecting the
  primary cursor's position within the file;
- windows still draw the scrollbar as part of drawing a window, with
  track, thumb, and cursor highlight as described above;
- `win:scrollinfo()` keeps its name, its six integer fields, and the
  values it reports for a given window state, as documented in the
  embedded API documentation;
- the screen-row text object still covers exactly the bytes of the
  displayed row containing the cursor position (plus the continuation rows
  of the same wrapped file line, bounded by the viewport), and reports an
  empty range for positions outside the displayed area;
- all other editor behavior, existing keybindings, and the public API used
  by this tree's tests and scripts keep working.

## Verification

Build as usual (`./configure && make -j4`, which should stay warning-free)
and run the core suite with `make -C test/core`; every test must pass, before
and after your change. Please also start the editor on a couple of files
(startup commands are supported, e.g. `./vis +q file.txt`) and check that the
status bar and the scrollbar still render. The change should be carried by
reorganization, not by hiding behavior behind shapeless wrappers: whoever
reads the code afterwards should find _less_ duplicated viewport logic, not
more.
