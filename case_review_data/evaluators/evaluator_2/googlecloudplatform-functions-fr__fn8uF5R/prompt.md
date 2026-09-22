# Consolidate the per-instance concurrent invocation admission control

## Observed

The Node.js functions framework in this repository gates how many
user-function invocations a single instance runs concurrently. A boot-time
platform setting supplies the cap as a positive integer; requests beyond the
cap are held and resume, oldest first, once an in-flight invocation settles.
When the setting is absent (or not a usable non-negative integer) the
framework must behave exactly as it did before the capability existed.

The gate works, but the capability grew piecemeal and has no owner.
The functionality that implements it is spread as small fragments across the
request-handling code:

- Request pipeline construction parses the cap setting itself, and locally
  defines the gate middleware that admits or parks each request, a helper
  that releases a settled request, and the marker each admitted request
  carries on its response.
- The module that holds cross-request state for crash reporting also owns
  the gate's counter and its parked-request queue, exported as mutable
  shared state that other modules mutate directly.
- The user-function signature wrappers contain a second copy of the release
  logic and invoke it on the callback-completion path and the crash path,
  because event-style functions and thrown errors never pass back through
  the pipeline's own termination handling.
- The socket-timeout middleware inlines its own release copies inside its
  inactivity-timeout and connection-close event handlers, as the safety net
  for requests that never settle.
- The process entrypoint parses the same setting twice more, once to report
  a malformed value at startup and once to print the effective cap in the
  startup banner.

Because these pieces coordinate indirectly (shared mutable state, a response
marker, response-completion and socket-event callbacks), the capability
cannot currently be understood, tested, or changed from one place.

## Task

Investigate the affected request-handling subsystem end to end and rework it
so the concurrent invocation admission capability is owned by a single
module: parsing, validation, and defaulting of the cap setting; the
bookkeeping of in-flight and parked requests; admission, parking, and
resumption; and the release performed on every termination path should be
defined in one location. Request-handling code elsewhere should interact
with the capability only through that owner's API, not by re-implementing or
directly mutating parts of it.

Address every instance of this same underlying issue across the components
you find — completing only part of the scope leaves the next change as
scattered as today. Remove obsolete fragments rather than leaving dead
duplicates behind.

## Behavior that must be preserved

- No setting configured (the default): fully concurrent behavior identical to
  before the capability existed — nothing parks, nothing queues, no new
  output.
- A malformed or negative value: the instance stays ungated, one error
  report is emitted at startup naming the setting, and the server still
  starts and serves.
- A valid positive integer `N`: at most `N` invocations run concurrently on
  the instance; further requests are held and admitted oldest-first when a
  slot frees.
- A settled request releases its slot exactly once, on every termination
  path: normal completion, callback completion, thrown or rejected errors and
  the uncaught-error crash path, client disconnect, and socket inactivity
  timeout. No path may leak a slot (an instance would eventually wedge at
  capacity) or release twice.
- The startup banner keeps its existing lines, wording, and order; the line
  reporting the concurrency level appears only when a valid positive cap is
  configured.
- The pre-existing timeout and abort machinery for requests is unchanged.
- No new runtime dependencies.

## Acceptance

The repository must build and the full existing test suite must pass without
modifying existing test expectations (adding tests is welcome). The
capability must remain fully implemented and effective — do not remove,
bypass, or disable it to simplify the rework.
