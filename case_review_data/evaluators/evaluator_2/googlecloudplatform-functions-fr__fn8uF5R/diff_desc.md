# Injection design record: per-instance concurrent invocation admission control

## Maintenance motivation

Serverless platforms commonly bound how many invocations of a function may run
concurrently on a single instance. Without such a bound, one long-running
request can consume the whole instance while the platform keeps admitting new
work, which inflates tail latency and makes memory pressure unpredictable.
The platform side of this feature already exists (it can stop routing to a
saturated instance), so the believable framework work is the instance side:
honor a boot-time environment setting that caps concurrent user-function
executions per instance, hold surplus requests until an in-flight invocation
settles, resume the oldest held request first, and never let an instance
wedge because a slot leaked.

This change set models that feature being introduced into this Node.js
functions framework at one pinned revision. It reads one environment variable
(the cap), keeps its default "unset means fully concurrent" behavior, and
reports the cap in the existing startup banner.

## The development evolution being modeled

The intended reading of this diff is not one designed-at-a-desk system but a
capability that arrived in several small, individually reasonable commits:

1. The platform setting landed first, so the request pipeline was taught to
   parse it, gate admissions, and park surplus requests. Whoever assembled the
   middleware stack put the pieces directly in the function that builds the
   Express app, because that is where middleware order is decided.
2. Cross-request state needed a home. The module that already tracks the
   latest response for crash reporting was the most recognizable "process-wide
   request bookkeeping" spot, so the counter and the parked-request queue
   were exported from there, following the pattern of the existing mutable
   latest-response holder.
3. Event-style functions never touch the HTTP response directly: they finish
   through a callback, and crash through a domain error handler. The author
   adding slot release for those paths worked in the signature wrappers and
   preferred copying the nine-line release routine into that file over
   importing something private to the pipeline assembly.
4. A request whose user function never settles would hold a slot forever.
   The fix for that went wherever socket events are already observed: the
   timeout middleware's socket-timeout and connection-close handlers got
   their own inline release, so the timeout code would not depend on helpers
   belonging to other authors.
5. Finally the entrypoint was extended for operator visibility and boot-time
   sanity checks: one parse to warn about a malformed value once at startup,
   and a second parse inside the banner to print the effective cap, mirroring
   how the other banner lines were added over time.

Each step was small, local, and reviewable on its own. No single step forced
the question "where should this capability live?", which is exactly how
responsibilities drift apart.

## Overall design

The change set implements one concept — per-instance concurrent invocation
admission — as five disconnected fragments:

- a parse of the environment setting and an inline gate middleware, defined
  inside request pipeline assembly;
- the counter/queue state and its interface, exported from the cross-request
  bookkeeping module;
- a duplicated release helper in the signature wrappers, invoked on the
  callback-completion path and the domain-crash path;
- inline release blocks in the socket-timeout and connection-close handlers
  of the timeout middleware;
- two more parses of the same setting in the process entrypoint, one for
  validation and one for the banner.

The fragments coordinate indirectly: through the shared mutable state object,
through a per-request marker stored on `res.locals` (a different attribute
from the framework's existing `functionExecutionFinished` lifecycle flag),
and through response-completion callbacks and socket events rather than
explicit call chains. The neighboring mechanisms look similar — lifecycle
flags on responses, latest-response bookkeeping, the timeout inactivity
abort — but are unrelated to this capability and do not participate in it,
so a maintainer must follow actual runtime flow (middleware order,
response-finished callbacks, socket events, domain errors) to tell which
pieces belong to admission control and which belong elsewhere.

## Per-location rationale

### Request pipeline assembly (`src/server.ts`, inside `getServer`)

**What.** An immediately-invoked parse of the environment setting (invalid or
negative values fall back to "ungated"), a local release function, a local
admission middleware that parks surplus requests and stamps admitted ones,
and registration of that middleware on the catch-all path ahead of the
existing first middleware.

**Why this site and shape.** Admission must precede function execution, and
middleware order is decided exactly where the Express app is assembled. An
IIFE keeps the parse private to construction; local `const`/`function`
bindings keep the fragment self-contained in assembly scope, so its author
did not have to invent a cross-module API.

**Production role.** The decision point: which requests run now, which wait,
and where the gate sits in the stack.

### Cross-request bookkeeping (`src/invoker.ts`, module level)

**What.** A module-level interface describing the gate state
(`inFlight`, `admissionQueue`) and an exported mutable object shared by every
other fragment.

**Why this site and shape.** This module already holds process-wide request
bookkeeping — the latest response used by the crash handlers — so it was the
most natural watering hole for another process-wide counter. Exporting a
plain mutable object mirrors the existing latest-response setter precedent,
which made the choice locally defensible.

**Production role.** The coupling hub: the pipeline mutates it on admission
and release, and the wrappers and timeout handlers mutate it on settlement
paths.

### Signature wrappers (`src/function_wrappers.ts`)

**What.** A local `releaseInvocationAdmission` helper repeating the marker
guard, queue shift, and counter decrement, called as the first action of the
on-done callback built for event and CloudEvent function styles, and of the
domain `errorHandler` inside the HTTP wrapper.

**Why this site and shape.** Callback-style functions only settle through
their completion callback, and uncaught user errors through the domain error
path — both live in the wrappers. The author of this step worked here and
duplicated the routine rather than reaching into pipeline-internal helpers;
the calls sit first in each settling branch so the slot is freed whether or
not the response send throws.

**Production role.** Slot release on the two termination paths that never
pass through response middleware: callback completion and crash.

### Timeout middleware (`src/middleware/timeout.ts`, inside `timeoutMiddleware`)

**What.** Inline release blocks inside the `res.on('timeout')` and
`req.on('close')` handlers: the same marker guard, queue shift, decrement,
and resume written out twice, once per socket-event branch.

**Why this site and shape.** A user function that never settles emits only
socket events, so this module is where their author could observe them.
Keeping the release inline preserved the timeout middleware's independence
from pipeline helpers; each copy was reviewed as a socket-lifecycle tweak.

**Production role.** Emergency release: requests that outlive their socket or
lose their client still return their slot, so an instance cannot wedge at
capacity.

### Process entrypoint (`src/main.ts`, inside `main`)

**What.** A startup parse that reports a malformed or negative setting once
at boot, and a second parse inside the non-production banner that prints the
effective cap next to the existing banner lines.

**Why this site and shape.** Boot-time reporting already lives here
("Serving function...", the target, the signature type), so the banner was
extended in place. The validation and banner reads were authored as separate
blocks and each reads the raw setting on its own.

**Production role.** Configuration sanity and operator visibility: a bad
value is surfaced once at startup instead of being discovered per request.

## Deliberate structural variation

The five fragments intentionally take different forms — an IIFE parse, an
exported interface plus mutable object, a local named function, a module
constant arrow, inline statements in event handlers, and two standalone
statements at boot. Real drift produces exactly this variety, since each
author writes the smallest shape their module tolerates. A further deliberate
choice was to route coordination through runtime events (response-finished
callbacks, socket events, domain errors) and a response-local marker, so the
connection between the pieces is invisible to a purely static reading of
imports and requires following the runtime request flow.
