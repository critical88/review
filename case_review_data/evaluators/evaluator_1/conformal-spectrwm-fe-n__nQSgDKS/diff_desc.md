# Injection design record - window lifecycle work unrolled into the phase drivers

## Maintenance motivation

The change models a window-management debugging session in spectrwm, a
single-file tiling window manager where every piece of window state lives in
`struct ws_win` and every phase of a window's life is driven by a handful of
top-level driver functions.

Two reports landed in the same week: windows occasionally reporting a stale
`WM_STATE` after being re-managed on startup (the very issue the tree's latest
upstream commit series was chasing), and focus cycling behaving erratically
during rapid keybinding storms on multi-transient clients (focus landing on a
hidden redirect target, urgent windows skipped, descendent windows sinking
below their main). Both investigations live in exactly the paths this diff
touches: the region mapping refresh that decides which windows a region shows,
the keybinding dispatcher that resolves the next focus, and the handler that
hides a window and settles its replacement focus.

Debugging those interactively in a stepping session is painful when each
transition is split across a chain of small per-window helpers: to watch one
map/unmap you hop between the walker, the map helper, the state-stamping
helper, and the focus bookkeeping; to watch one hide you jump between the
handler, the focus-drop helper, the EWMH flag helper and the property-rebuild
helper. The natural thing to do at 2 a.m. with a debugger attached is to
unroll those calls into the driver so the transition reads as one continuous
block - and that is the evolution this diff captures, including the part
where the session ends, the bug gets chased down elsewhere, and the unrolled
version stays in the tree.

## Normal development evolution being modeled

This is a well-known shape of organic growth in C codebases with split
data/function responsibilities: behavior that belongs to a record gets
re-implemented inside the higher-level function during an emergency
investigation, because "seeing all of it in one place" beats "the code is
beautifully factored" while you are one breakpoint away from a reproduction.
The code remains comment-dense in the places where whoever unrolled it left
notes to themselves about what a step does and why it is safe - the same
notes they would have written in the original helpers, now attached to the
inlined copies. The original helpers still exist because other callers never
stopped using them; some of them simply gained a second, in-frame copy inside
one caller. That
"second copy in exactly one caller" lifetime is the realistic signature of
this evolution, and the diff keeps the surviving helpers and their other
callers untouched so the two-copy situation is visible to future maintenance
rather than hidden by deletion.

## Overall design

Everything happens inside `spectrwm.c`, inside three existing top-level
driver functions. No function is added, removed, or renamed; no `struct`
definition, no public entry point, no config or protocol surface changes.
The transformation is statement-level: bodies of per-window helpers are
re-expressed inside the drivers, with guard chains flattened into
equivalent `if`/`else if` sequences that preserve the exact evaluation
order and the short-circuits, and loop-local bookkeeping hoisted to the
top of the driver so the unrolled steps read as one block. Debug logging
is retained in the unrolled copies with the same debug statements the
helpers carried - the point of the exercise was to see the log lines while
stepping. A few pointer locals are cached once per driver invocation (the
workspace and bar of the region being walked, the screen's regions, the
iconified head) so the unrolled code does not re-dereference for every
single step. Within the loops, windows that fail the visibility or
relatedness tests are skipped by the same guard shapes the walker used
before.

The unrolling is constructed as pure relocation: no conditional is
re-ordered, no X request is added or dropped, all counter updates and
property writes stay at the same points, and every new statement inherits
the project's `-Wmissing-prototypes -Wall -Wextra -Wshadow` hygiene.

## Cluster rationale

### 1. Mapping refresh walk - `update_region_mapping`

**Site selection.** This function is the per-region phase of the mapping
refresh: for the region's current workspace it decides, window by window,
which windows must be shown and which must be hidden. It sits directly on
top of the per-window map/unmap machinery, and it was the primary suspect in
the stale-`WM_STATE` investigation (the reported symptom is precisely "window
is mapped but its state property says otherwise"). That makes it the natural
first place someone unrolls: the walk is where the bug must be reproduced.

**Shape.** The unroll replaces each `map_window(w)` / `unmap_window(w)`
call with the transition written out: the `XCB_WINDOW_NONE`-frame skip, the
`xcb_map_window`/`xcb_unmap_window` sequence over the frame, the client
window and the debug window, the `mapping`/`unmapping` counters, the
`mapped` flag, and the `xcb_change_property` restamp of `WM_STATE` as
`NORMAL` or `ICONIC` on the client. The relatedness test that decides
whether a window belongs to the moved family (`main` identity against the
seed window, with the dock special case) is likewise evaluated inline next
to the walk conditions instead of through a helper predicate.

**Production role.** Once invoked from the root region walk and again per
dynamic region (`update_mapping`), this code is what synchronizes what is
on screen with what the layout thinks should be - the path exercised at
startup, on workspace switches, on every region/layout change and on every
focus storm that crosses regions.

### 2. Focus cycling dispatch - `focus`

**Site selection.** The keybinding dispatcher is where all five focus
movements resolve their next candidate (cycle forward/backward, previous,
urgent, free). The focus-storm report landed here: candidate selection is
the only place that touches the redirect chains of several windows in one
breath. Unrolling here makes each binding path readable as one block,
which matches how the storm was being stepped through.

**Shape.** Every path's resolution steps are written out in the driver:
seeding from the current workspace's focus or its previous focus (and the
same seeding against the root workspace on the free path); following a
candidate's `main` into its `focus_redirect` chain, bounded by the
destination workspace's normal-window count and stopping on the first
NULL, hidden or invalid target; the urgency probe, inlined as the quirks
gate (`SWM_Q_IGNOREURGENT`) followed by the ICCCM urgency bit and the
demands-attention check; and, after focus is handed over, the family
re-stack, inlined as the layer decision chain (raised, desktop, below,
fullscreen, maximized, dock, floating, else tiled) applied to the main
window and then to every related window of the same `main`, with related
windows still pinned at-or-above their main's layer. Existing driver-level
calls that coordinate whole screens - unfocus application, window counting,
stack refreshes, the switching of workspaces - remain as calls.

**Production role.** This is the backbone of keyboard-driven window management: every
focus binding the user can press funnels through this function, which
decides the candidate, the workspace and the stacking consequences of the
move.

### 3. Hide (iconify) transition - `iconify`

**Site selection.** The hide path executes two coupled state changes on one
window - dropping its focus and flipping its EWMH hidden state - and both
halves were under suspicion in the storm week (windows reappearing on the
iconified list or keeping stale `_NET_WM_STATE` atoms). The transition was
the third one unrolled, because both state flips must be watched together
to understand a single hide.

**Shape.** Three blocks were brought into the driver. The focus drop:
the validation ladder for the targeted window and its workspace
(unmapped / invalid win / invalid ws / else the drop itself), with the
screen focus mirror and the workspace `focus`/`focus_raise` mirrors
cleared under the ladder's final rung, dangling refs cleaned before
anything else, priority-raise work (stack and layer refresh) only when
the raise demands it, and the minimal-border floating check with its
border-shift written out. The EWMH flip: computing the pending/changed
words from the flag field, inserting into or removing from the screen's
iconified head through the window's list entry, the geometry reload/store
moves for floating windows on unhide and on the above/below/maximized
transitions, the attention-frame redraw, and the `EWMH_F_MAXIMIZED`
clearing under its condition. The state property rebuild: the atom-set
assembly from the window's full flag word in the established order,
published (or withdrawn) against the client window. A pointer to the
screen's iconified head is cached once per call so those steps read as one
unit.

**Production role.** This handler is the user-facing hide action; it also
decides the replacement focus, and its pointer-follow/static split is kept
exactly as the surrounding driver had it.

## What the drivers look like after this change

Within each driver, the scope-level decisions are still visible at the top
and the bottom - which region, which workspace, which binding path, which
seed wins, which focus is finally handed over - but the middle is now a long
window-by-window chronicle of everything that used to happen one helper call
away: identification, transition, state stamping and list bookkeeping all
executed directly on the windows' fields. The three drivers read as
narratives of a window's life rather than as coordination code, which is
exactly what the debugging session wanted and exactly what later
maintenance has to live with.
