# Injection design record — parked truncation-safe formatting experiment (issue #45 follow-up)

## Maintenance motivation

The pinned revision of this library is itself the landing of an `snprintf` semantics
fix: `snprintf_`/`vsnprintf_` now report the number of characters that *would* have
been written, the value a caller needs to detect truncation (issue #45). The natural
next question for an embedded formatting library is the inverse: *how much space
will this format need before I hand it a buffer at all?* A length-only query would
let callers size stacks and buffers ahead of time, and an internal account of the
rendered length of every conversion would let the engine prove that it never
silently truncates.

This change models the developer starting exactly that piece of work — a
"truncation-safe formatting" experiment — at the point where progress on other
fronts stops, the branch is parked, and later work resumes on top of the tree
without the experiment ever being wired up or finished.

## Modeled development evolution

The modeled history has three phases, mirroring how such an experiment usually
grows inside an existing formatting engine:

1. **API seed.** The wanted feature is a dry-run length query in the spirit of
   `snprintf(NULL, 0, ...)`, so a matching public prototype plus its contract
   documentation is drafted first in the header — the established habit in this
   codebase, where every public routine is declared with a `\param`/`\return`
   block. The implementation is deferred.
2. **Plumbing and estimators.** Experimentation goes bottom-up inside the
   translation unit: a counting sink that reports rendered characters instead of
   storing them, a dry-run wrapper that drives the existing engine through that
   sink, per-family capacity estimators developed next to the real conversions
   they are meant to guard, and a module-level high-water mark meant to become
   the reported state. Each piece references only existing, live primitives, in
   the same naming style as the rest of the file.
3. **Instrumentation.** To connect estimators with reality, the live
   conversion functions and the core parse loop are instrumented with
   length-accounting locals, and a temporary trace emission is added to the
   parse loop, immediately disabled behind a literal guard so it stays quiet
   while other work continues.

Then the work stops: the reporting that would have consumed any of this is never
written, the new prototype is never implemented, and nothing dispatches into any
of the plumbing or estimators. The tree continues to build and pass its suite
exactly as before — that is the state the next maintainer inherits and the state
this case adds to the repository.

## Overall design

The additions are scattered the way such a half-finished experiment genuinely
scatters, rather than confined to one corner:

- the public header gets one new prototype block;
- the module state region and the output-strategy plumbing get the watermark and
  the counting sink;
- the integer, float, and string specifier regions each get a capacity
  estimator next to the live implementation;
- the integer formatting stage, the float conversion, and the core parse loop —
  live, hot code — carry the accounting instrumentation and the disabled trace;
- a dry-run wrapper sits beside the core engine entry.

Total surface: two files, one translation unit plus the header, purely additive
(insertions only, no existing line modified), CRLF line endings preserved, and
comment/naming conventions matched throughout. No test was touched.

## Per-cluster rationale

### 1. Public-header API seed

**What:** a documented prototype for the planned length-only query, declared
after the existing function-pointer formatting routine.

**Why there:** this is where every public entry of this library lives, and
recording the intended contract first is the file's own convention (`\param` /
`\return` blocks next to each declaration). The doc comment honestly notes the
contract is not implemented yet and points at the existing truncation-reporting
semantics as the interim behavior.

**Production role:** the intended public face of the experiment. Declaring it
before implementing it is exactly the harmless-looking step that later leaves
an unbacked declaration in the shipped API.

### 2. Module accounting state

**What:** a file-scope watermark counting the longest formatting result seen,
with a comment naming it as the accounting's report target.

**Why there (form):** next to the output plumbing where all other shared
definitions of the engine sit; it is the "collect the number the experiment
produces" half of the accounting, so it belongs with the machinery it was meant
to observe. A single `size_t` in the file's naming style is what a first
working-draft version of such state looks like — there is no setter or reader
yet because the shutter never exists.

**Production role:** intended reporting state. Its presence promises a
mechanism to the reader that the rest of the code never builds.

### 3. Counting sink and dry-run backend

**What:** a static output-character callback that stores the running count
instead of characters, signature-compatible with the existing sink type, placed
directly after the live sinks; and a static wrapper that invokes the existing
formatting engine with that sink and returns the count.

**Why there (form):** the engine is already polymorphic over output strategies —
buffer, user character sink, function-pointer - through one shared signature.
Reusing that signature is the honest way a dry-run mode gets prototyped here:
from the parser's point of view the sink is indistinguishable from a real one,
so a length-only render requires no engine changes. The wrapper is the obvious
connector object: existing engine, pass the new sink, return the total.

**Production role:** the dry-run core of the feature experiment. The sink looks
like a fully-paid-up member of the output plumbing (and its signature says it
could be dispatched by the engine), but the only thing that does dispatch it is
the wrapper — and nothing dispatches the wrapper. Both of them were designed to
be the experiment's proof-of-concept, so neither was ever published.

### 4. Per-family capacity estimators

**What:** three small static helpers, one next to the string specifier support,
one after the integer formatting stage (inside the same feature-flag region),
one after the float conversion: how many characters a `%s` needs including the
terminator and width padding, how many digits/characters an integer conversion
renders including precision, prefix, sign, and width, and an upper bound for a
fixed-notation float conversion.

**Why there (form):** the pre-flight capacity question decomposes by conversion
family (the same decomposition the engine itself uses), so the prototypes grow
next to the family they reason about. Each helper is deliberately small and
self-contained: it calls only existing live primitives (the secure string
length, plain arithmetic), it reads naturally as a candidate pre-flight
computation independent of the accounting instrument, and its comment frames it
as a future acting input to the truncation checks — how the draft author
intended to keep the plan visible in the code.

**Production role:** intended pre-flight/session sizing for callers and an
independent cross-check on the accounting. As parked, each is an uncalled
arithmetic tree that looks live only in the sense that a reader can compile it
in their head.

### 5. Instrumentation inside the live conversion paths

**What:** a `conv_len` local in the integer formatting stage, in the float
conversion, and a total-length local in the core parse loop, in each case
initialized from real values and stored at the natural reporting point (before
reverse emission, per parsed specifier, and at loop end), each with a short
comment naming the account as that function's report to the accounting hookup
of issue #45.

**Why there (form):** those two attributes are where the rendered length is
actually known for a conversion, and the core parse loop is the place where a
whole-format total exists and the only thing that touches all conversion
families from one vantage. Instrumenting live code — instead of adding to dead
scaffolding — is what makes each of these entries a real maintenance hazard
rather than a pile of unbound helpers: every retired store reshapes the
accounting the next reader is told about in the comments, and only careful
deletion can remove the accounting without touching the surrounding logic.

**Production role:** intended data feed for the never-finished reporting. As
parked, the `conv_len` locals are pure extra state to reason about (and each
`conv_len = ...;` a no-op) inside every formatting pass.

### 6. Disabled parser trace

**What:** a three-line block in the middle of the parse loop that emits the
current parse position to the character sink, guarded by a literal `if (0)`
with a comment about chasing the issue #45 truncation reports.

**Why there (form):** a trace of *which specifier* is being handled belongs in
the parse loop, which is the only place that knows the parse position. Guarding
experimental output with a literal `if (0)` rather than deleting it is a common
working habit while a feature misbehaves and is being chased: it keeps the
probe sitting at the exact spot where parsing still felt wrong, ready to be
switched back on. Left in place through a code drop, it becomes the classic
unreachable-statement accident — disabled debug scaffolding that rides along in
shipped code because the author meant to return to it and never did.

**Production role:** temporary observability for the experiment; as parked it
is a statement in the middle of the hottest loop of the engine which never
executes and every reader must step around.

## Design fidelity notes

- Every addition follows the file's existing conventions: the leading
  underscore-prefixed internal naming, one-purpose small static helpers, block
  comments written as plain prose sentences, and CRLF line endings throughout.
- Nothing existing was modified: the patch consists purely of insertions into
  the two production files, so the shipped header and engine text are only
  added to, never rewritten.
- The comment trail (repeated references to the truncation-safe work and
  issue #45) is written to be a plausible residue of the honest habit: document
  the half-done plan in the code at every junction — the residue a maintainer
  actually must read past afterward.
