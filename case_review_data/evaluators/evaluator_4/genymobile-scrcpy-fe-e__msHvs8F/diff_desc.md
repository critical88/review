# Injection design record — scrcpy client input/capture display-state work

## Maintenance motivation and modeled evolution

The scrcpy client renders the device display in an SDL window and translates
every pointer event from window coordinates into device frame coordinates
before injecting it. In the pinned state, that translation lives in one
place: the display module (`app/src/screen.c`, `struct sc_screen`) owns the
window, the on-screen content rectangle, the client orientation and the
frame size, and exposes a conversion entry point through its public header.
The input manager consumes that entry point.

The change modeled here is a realistic piece of developer friction: input
handling needs *more* display context than a single conversion call provides.
A maintainer under time pressure (adding the "double-click on the black
borders resizes to fit" interaction, the Ctrl/Shift virtual-finger emulation,
and the relative-mouse capture integration) repeatedly needs the content
rectangle, the orientation, the frame size and the window mode. Instead of
extending the display class each time, code that accumulates in the input
and capture modules starts to read that state directly and re-derives the
answers locally. The dependency direction is still "downward" (input
consumes display), so nothing looks broken while it accretes, but it drifts
apart from the original layering: the input module becomes display-state-aware,
and behaviors that compute on the window-to-frame relationship exist in two
places at once, forcing every future display change to be mirrored by hand.

## Overall design

Four production files participate:

- `app/src/input_manager.c` — three new module-local helpers, each with a
  different shape, wired into the four pointer-processing paths (position
  query, motion, touch, mouse buttons);
- `app/src/mouse_capture.c` and `app/src/mouse_capture.h` — the capture
  subsystem stops receiving the bare `SDL_Window` and instead receives the
  whole display object, and its event loop gains a local relative-mode gate
  reading the display state;
- `app/src/screen.c` — the owner module passes itself to the capture
  initializer and drops its own pre-filter from the event loop delegation.

The instabilities intentionally take different forms so the diff does not
boil down to one repeated edit: a full copy of displaced coordinate
arithmetic, a composed mirroring query, an isolated decision predicate, and
an architecture-shaped re-wiring where a gate over the display state replaces
a gate in the owner.

## Cluster 1 — duplicated window-to-frame conversion (`input_manager.c`)

**What changed.** A helper converting a window-space point into frame space
was added to the input module, containing the content-rectangle
normalization and all eight client-orientation cases taken over verbatim
from the display module. The position query (`sc_input_manager_get_position`)
and the touch path (`sc_input_manager_process_touch`), which previously
called the display-module conversion entry point, were rewired to the local
copy. The mouse-motion path previously called the same public entry point and
was rewired too, but with a deliberate difference (see "Deliberate
variation" below).

**Why this site and shape.** The transformation is the canonical
"interested in someone else's data" artifact: it cannot be computed without
the content rectangle, the rectangle's relative position inside the window
and the current client orientation, all of which live on `struct sc_screen`,
while the input module's own state (which it touches constantly for key
code mapping, modifiers, mouse processor dispatch) contributes nothing to
it. Copying the math into the input module is also exactly how this drift
happens for real — the code still compiles, still works, and is now
invisible to designers. The update site count here is deliberately three
different production paths, the concrete mileage of the feature envy: the
motion, touch and query paths all now run through the local helper.

**Production role.** This is the core pointer-input navigation math of the
application; every touch event and mouse motion passes through it.

## Cluster 2 — implied copy: containment and virtual finger
(`input_manager.c`, mouse-button path)

**What changed.** Two more module-local helpers joined the button handler
of the input manager:

- a predicate deciding whether a window point lies outside the displayed
  content rectangle, replacing the handler's local comparison that kept the
  rectangle in one pointer variable; it now reads the rectangle fields
  directly on the display object, in the input module;
- a query computing the virtual-finger position for the "Ctrl/Shift +
  right-click injects a mirrored touch finger" emulation: it re-uses the
  local conversion and then mirrors the result against the frame size,
  replacing the handler's previous sequence of calling the public
  conversion and composing with a generic standing helper.

**Why this site and shape.** The double-click-on-borders resize path and the
virtual-finger emulation are two behaviorally distinct production roles of
the same display-state reading: one is a boundary decision (needs the
content rectangle), one is a composed geometric query (needs the frame size
in addition to the conversion). Together with cluster 1 they give the input
module three helpers of genuinely different liveness: raw displaced math,
composition, and pure predicate. The button handler is also correct as the
wire-up site because it is the path with both behaviors, so no additional
touch point was invented merely to raise a count.

**Production role.** The resize-to-fit-on-double-click interaction and the
virtual-finger emulation are user-facing scrcpy conveniences documented in
the user manual.

## Cluster 3 — capture subsystem pulling in the whole display object
(`mouse_capture.c`, `mouse_capture.h`, `screen.c`)

**What changed.** The mouse-capture component's dependency was widened in
its header: instead of holding the `SDL_Window` handle it was constructed
with, it now holds a pointer to the whole display object. Its event handler
gained a local check that inspects the display state — asserting that the
exposed window is the one it manages and that its input pipeline is wired —
and then evaluates whether the input pipeline requested relative mouse mode,
returning early (forwarding the event) when not. The owner module's event
loop, which previously pre-filtered this condition itself before delegating,
now relies on the capture component running that gate internally; the
initializer in the display module passes the display object instead of the
window handle. The window-handle-based setters inside the capture subsystem
now reach the window through the display object pointer.

**Why this site and shape.** This is how such a dependency inversion looks
when it arrives as an architectural request ("the capture decoration should
be self-contained about when it is active") rather than as lazy copying,
which is what makes this cluster the architecturally-flavored piece of the
diff. The capture subsystem now reasons about state that is entirely not its
own: mode lives on the input pipeline behind the display object's embedded
input manager, plus the display object's own integrity demands are now
asserted by a foreign module. The gate duplication also has a concrete
maintenance cost that a maintainer can notice from the diff alone: the
owner still keeps an equivalent helper for its two initialization-time uses,
and the two checks must now stay synchronized by hand.

**Production role.** Mouse capture is the relative-mouse-mode submode
(shortcut-key toggled, bounded by window focus) used by games and
pointer-locking applications.

## Deliberate variation and non-targets

- The motion path was replumbed to the local conversion helper but **still**
  composes with the standing generic `inverse_point` helper and keeps the
  same handler structure as before; only the conversion call changed. This
  site therefore looks similar to the others without being another full
  copy.
- The handler sites are intentionally heterogeneous: one passes a bare
  x/y pair through a small predicate, one passes the raw event coordinates
  plus two flags into a composed query, one swaps a compound struct
  initializer field. No single rewrite rule covers them.
- The rewritten mouse-button path drops its previous local pointer variable
  over the display rectangle; nothing new was added around it — the
  replacement mirrors the preceding structure one comparison at a time.
- Unchanged lookalikes that a reviewer might reasonably probe: the fps
  counter switching and the pointer-state logging in the input module still
  read the display object, but at a scale where their own module state
  dominates; the display module's own functions remain inside their owner
  file; the main wiring module passes the display object by pointer only.

## Behavioral intent

No user-visible behavior is intended to change: window-to-frame conversions,
the resize-to-fit exclusion test, the virtual-finger mirrors, and the
relative-mode capture semantics are each preserved verbatim in this diff.
The modeled evolution is a layering drift rather than a defect: the same
display-state interpretations now exist in several places at once, so future
work on the window geometry, the orientation mapping or the capture
semantics must update each copy by hand, and the capture activation
condition is now decided twice — inside the component and in the form the
display module still keeps for its own uses.
