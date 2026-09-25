# Injection design record — fantoccini action-source and command-plumbing change

## Maintenance motivation

fantoccini's action builders let users assemble input sequences tick by tick. At the
pinned revision a builder chain is written in two dialects: every input source can
`pause()` and `then(action)`, but the event you may queue depends on which source you
hold — key events are typed through a `KeyActions`-specific vocabulary, pointer events
through a pointer-specific one. Users who switch devices mid-chain (press a key, then
click, then scroll) have to re-learn the call surface per source type and drop down to
the raw action enums for anything the builder does not expose.

A natural maintenance request that follows from this is a **unified device vocabulary on
the input-source trait**: one fluent surface (`key_down`, `key_up`, `pointer_down`,
`pointer_up`, `move_to`, `scroll`, plus `tap`/`type_key` shorthands) that reads the same on
every source. The upstream WebDriver spec itself is organized this way — input sources
have *kinds* (key, pointer, wheel) — so a unified trait is a plausible API-growth
decision, not an exotic one.

The second, independent maintenance thread is vendor-routed deployments: running
fantoccini against a Selenium grid or cloud browser farm eventually needs custom
commands that hit a vendor endpoint or carry vendor headers. A plausible first step for
that feature is a trait that every WebDriver command can be routed through, so the
request path can consult it per command.

## Modeled evolution

The change models a maintainer unifying the builder surface in one sitting: the trait is
widened first, and then each existing source is brought up to the new trait one impl at a
time, giving each source real bodies for the members it can genuinely perform and
accept-and-drop bodies where the device has nothing to queue. The same commit adds a
small end-to-end exercise of the new surface, because API work of this kind would not
land without one.

The vendor-routing thread is modeled at its "scaffolding first" stage: the trait and the
request-path integration land together, while the only implementor at this revision — the
internal wrapper around WebDriver commands — defaults to no vendor routing and no extra
headers, since standard commands never need them.

Both threads are the kind of work a reviewer would expect to need follow-up design on:
the vocabulary members have not yet been attributed to the devices that can carry them,
and the vendor members have not yet been subdivided from the plain WebDriver command.
Nothing in the record below asserts how related or unrelated these threads are; they are
described per hunk so the relationships can be assessed independently.

## Overall design

- `src/actions.rs`: the input-source trait gains six required device members and two
  provided shorthands that chain them. Each of the six sources keeps its real `pause` and
  `then` members and gains an override per device member: a queuing body where that source
  actually produces that kind of event, and a body that accepts the arguments and returns
  the builder unchanged where it cannot.
- `src/wd.rs`: a new trait for vendor routing (URL override + transport headers) with
  reference and box forwarding impls, and the existing WebDriver command trait is
  re-based on it as a supertrait so every existing and future command carries the vendor
  surface.
- `src/session.rs`: the internal command wrapper implements the vendor members with the
  "no vendor routing" default (no URL override, empty header list), and the request path
  starts preferring an explicit vendor URL when present and attaches vendor headers after
  the user-agent header.
- `tests/actions.rs`: one new browser-driven exercise of the fluent surface, registered
  for both locally driven browsers.

## Per-location rationale

### `src/actions.rs` — trait widening

The trait documentation hunk states the unification contract: the same six members are
available on every source, and sources that do not produce a given kind of device event
accept its arguments without queueing an action. The two provided shorthands (`tap`,
`type_key`) are implemented entirely in terms of the six members, which is the cheapest
way to make the shorthand available everywhere at once, and matches how the existing
`pause`/`then` chain composes ticks.

Design choice: the members are required (no trait defaults). Required members make the
unification visible in every impl — each source must state its position on each member —
which is exactly what a "make every device one vocabulary" edit does.

### `src/actions.rs` — `NullActions`

The null source holds a real position name: it queues nothing by design (it exists to
contribute pauses), so all six members accept-and-drop. These bodies mirror the source's
existing identity and are given per-member comments consistent with the trait
documentation.

### `src/actions.rs` — `KeyActions`

Key sources queue real key events, so `key_down`/`key_up` forward into the existing
`KeyAction::Down`/`Up` payloads via the `then` chain. Pointer and wheel events are not
produced by a keyboard source, so those four members accept-and-drop.

### `src/actions.rs` — `MouseActions`, `PenActions`, `TouchActions`

The three pointer sources share the same shape: real pointer bodies (`pointer_down`,
`pointer_up`, `move_to` building the existing `PointerAction` payloads) and
accept-and-drop bodies for the key and wheel members. Giving the three sources the same
comment wording ("so that shared builders still type fluently") is deliberate: the
unified-trait story reads identically per device kind.

### `src/actions.rs` — `WheelActions`

The wheel source queues real scroll events into the existing `WheelAction::Scroll`
payload. Key and pointer events are not produced by a wheel source, so those five
members accept-and-drop.

### `src/wd.rs` — vendor routing trait

The new trait carries documentation describing when vendor routing applies (grids,
cloud farms, vendor consoles). Two blanket impls forward through references and boxes,
mirroring the two blanket impls the adjacent WebDriver command trait already carries —
that is the established ergonomics pattern in this module. The existing command trait
then grows the new trait as a supertrait, which keeps every command object routable
through the new members without touching any external command.

### `src/session.rs` — wrapper provision and request path

The internal wrapper around WebDriver commands implements the vendor members with the
neutral default for standard commands: no URL override (`None`) and an empty header
`Vec`, each with a doc comment pointing at the trait members to override for vendor
integration. In the request path, an explicit vendor URL takes precedence over the
session-base endpoint, and the header attach loop is placed immediately after the
user-agent block so per-command headers ride on the same request being built. The two
comment lines explain the precedence and the attach step in place.

### `tests/actions.rs` — end-to-end exercise

One new async test drives a real page through the fluent surface: it types two
characters using the key members and the typing shorthand, then clicks the alert button
using the pointer members and the tap shorthand, asserting the resulting page state each
time. It is registered in both the firefox and chrome modules, matching how every other
action test in the file is registered. The test exercises only the members whose device
kinetically performs each event kind, and pins the builder ergonomics the trait
documentation promises.
