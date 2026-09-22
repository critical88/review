# The "accept any handle" rework made every wrapper look like it does everything

Some context for whoever picks this up: this library models each COM
interface as a small wrapper struct, and the convenience package on top
contains the helpers people actually call to drive objects — calling methods
and properties by name, walking collections, hooking up event sinks. Until
recently those helpers only accepted the late-binding wrapper, so users who
already held a different valid handle had to re-query for the late-binding
interface themselves before a helper would take it. A little while back we
did a generalization pass to fix that: the helpers now advertise that they
take a handle from anywhere in the wrapper family. That part was the point,
and users did want it. The problem is the shape the fix took, and after a
release cycle of living with it we want the design reconsidered.

What we now see, day to day:

- The method list on *every* wrapper in the family is a dozen-plus entries,
  and most of the extras cannot honor the call. Call one on the wrong
  wrapper and you get an error at runtime — either a "couldn't hand out that
  interface" failure from a runtime re-query, or a plain not-implemented
  error — instead of the compile-time mismatch we used to get. The names on
  the wrappers now claim things the underlying COM interfaces never offered.
- The base of the whole wrapper family (the struct every other wrapper
  embeds) is where these catch-all bodies ended up, in **both** of its
  per-platform companion files. Some of them are real delegating
  implementations that re-resolve a different interface at runtime; some
  are pure error returns, because a bare handle genuinely cannot answer
  them (which connection point would "advise" even mean?). New contributors
  can no longer tell what the base handle is supposed to be responsible
  for; when we on-boarded a contributor last month, half their questions
  were "can I call this here?" and half the answers were "it'll compile,
  but don't".
- One of the richer wrappers, whose real job is late binding, acquired
  collection-cursor operations during the same pass — each call pulls the
  collection property and opens a fresh cursor to drive the operation. Fine
  in isolation, on-pattern for this library even, but there is no collection
  capability in its underlying contract and no internal caller depends on
  it. It exists so that wrapper's surface looked complete next to the
  others.
- Every helper now depends on the entire shared surface, and most helpers
  use two or three methods of it — one of them needs exactly one. The
  parameter type used to tell a reader precisely what a helper depended on;
  code review of call sites that used to take minutes now involves opening
  the helper source to check "wait, which of these does it actually use?".
  That dependency information is what we lost, even for the helpers whose
  behavior is pure convenience.

## What we're asking for

Sort the capability surface back out along the lines the real COM contracts
draw: a wrapper should be doing what the interface it wraps actually
provides, and the handle types helpers depend on should each cover exactly
what those helpers exercise — the smallest sufficient surface, whether you
get there with focused generic abstractions or by leaning on concrete
wrappers where one is the natural fit. Please take the sweep's leftovers
with you: unused broad handle declarations, and bodies whose reason for
existing was to satisfy them, shouldn't linger as dead surface afterwards
(both platform variants of the sources need to stay consistent).

## What must not change

- Programs that use our helpers with the concrete late-binding wrapper keep
  compiling unchanged and keep getting the same results and the same error
  behavior as before the episode — same dispatch semantics for methods and
  properties (including the by-reference property form), same results for
  calls that worked, and the same actual implementation code running under
  those calls as before.
- Collection walking still goes through the property objects publish, and
  the connection helper still wires itself through container/connection
  point lookups and sink registration —
  shrink what the parameters promise, not what the helpers do.
- Scope to the wrapper family and the helper package. Everything else —
  marshaling, safe-array handling, type/runtime inspection, the deprecated
  fluent API we're no longer touching, low-level syscall plumbing — predates
  this mess and should not need to change; keep low-level dispatch mechanics
  in the core package rather than duplicating them into the helpers.
- The repository's tests pass afterwards, and the sources stay consistent
  for both platform build configurations.
