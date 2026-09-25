# Injection design record — feature envy in patator

## Maintenance motivation

Patator is a multi-purpose brute-forcer built around one orchestration
engine. An attack run consists of producers handing candidate payloads to
consumers, consumers passing results back through a multiprocessing
manager, and the log service turning that queue traffic into console
rows, CSV/XML records, runtime files, and saved response dumps. Two
kinds of state sit underneath that loop:

* each attempt's **response object** — status code, message, size, timing,
  and (for HTTP) content length and parsed target — owned by the response
  classes;
* each worker's **progress record** — hit/done/skip counters, the current
  candidate, and the recent-seconds ring — owned by the progress class.

A plausible refactoring narrative: while preparing a release that promised
"unified response handling and progress reporting", a developer observed
that the response classes and the progress class kept sprouting small
methods whose only callers live in the orchestration layer. Wanting a
single place where maintainers could read how evaluations produce
verdicts and how counters move, that developer pulled those behaviors
**up into the callers** — into the controller and into free functions of
the reporting pipeline — reasoning that "the controller already owns the
run, so it should own how results are judged and tallied". The diff below
is that maintenance step.

## Normal development evolution being modeled

This is a routine, plausible mid-feature commit: keeping the multiprocess
plumbing untouched, the developer re-routes three existing
responsibilities. Response evaluation, which used to be answered by the
response objects themselves through `match*(key, val)` dispatch, becomes
a controller-side dispatch table keyed by condition name. Result
rendering and file naming, previously answered by the response classes'
own `indicators()`, become pipeline helpers that unpack response fields
externally. Progress bookkeeping, previously expressed as direct counter
manipulation at two call sites, becomes controller methods that read and
write a whole worker record. Each individual move reads as a
symmetry/cleanup change: fewer methods on data classes, one more
"complete" implementation next to the code that calls it.

No behavior is intended to change: conditions keep their operators and
anchoring, saved hits keep their naming and contents, the summary and
interactive monitor keep their exact rendering, and resume/skip/retry
accounting is byte-for-byte equivalent.

## Overall design

The transformation is confined to the framework core
(`src/patator/patator.py`) and deliberately crosses its three semantic
layers:

1. **Reporters before receivers** — two new module-level functions
   receive a response object from `process_logs` and unpack its fields to
   render a result row and to name a saved dump. The reporting pipeline
   now decides what response data means (`write_response_result`,
   `response_indicators`), with HTTP-vs-base variant selection done twice
   in two different styles (an `isinstance` branch and a `hasattr`
   probe), mirroring the kind of drift that happens when two helpers are
   written at different times.

2. **Orchestrator-owned evaluation** — `Controller.match_response`
   absorbs the condition dispatch that response classes previously
   performed on their own state: regex matching for `code`, range
   matching for `size`/`time`/`clen` (HTTP only), equality and substring
   and multiline/or-plain search on the message, each by consulting the
   response's attributes directly. Where a condition is not available for
   a response kind, the controller raises the same
   `match_<condition>`-shaped `AttributeError` that the old
   method-dispatch produced, so error text is preserved verbatim.
   `lookup_actions` delegates to it for every free/skip action.

   The response classes, in turn, are reduced: `Response_Base` loses its
   `indicators()` plus the whole `match`/`match_code`/`match_size`/
   `match_time`/`match_mesg`/`match_fgrep`/`match_egrep` evaluation
   surface, and `Response_HTTP` loses `indicators()`, `match_clen`, and
   its multiline `match_egrep` override. What remains on them is state,
   string rendering, dumping, and target formatting: they have become
   pure data owners.

3. **Orchestrator-owned accounting** — `Controller.record_progress`
   absorbs the consumer-report application that used to be inlined in
   `report_progress`: the skip path flips the worker's `skip_count`;
   resume-continuation arithmetic consults `done_count` and the
   controller's resume table; `current` and `seconds` are written on the
   worker; quit flips the global shutdown flag; result rows are logged;
   hit/fail/retry/ignore increment the worker's counters; saved hits are
   persisted and pushed to the stack of final hits. The remaining two
   controller methods cover the read side of the same state:
   `progress_snapshot` packs a worker's four counters for the final
   report, and `progress_detail` renders the per-worker line shown by the
   interactive `f` monitor. `Controller.fire` now iterates workers
   through `progress_snapshot` instead of summing attributes inline.

## Per-cluster rationale

**Cluster A — reporting helpers (module level, new).** The log service is
the natural place a developer looks when result rows look wrong, so the
code that answers "what did this response produce?" is placed right next
to it, as free functions taking the response object as a parameter. Each
helper needs exactly the response state that its consumer needs, so both
reach in for `code`, `size`, `time`, and the HTTP-only `content_length`.
Because the row renderer and the file-namer were grown separately, one
uses an explicit `isinstance(resp, Response_HTTP)` branch and the other a
`hasattr(resp, 'content_length')` probe — realistic drift between two
helpers that answer the same "which variant is this?" question.

**Cluster B — controller-side condition evaluation.** `match_response`
is the heart of the old `match_*` dispatch merged into one function
parameterized by `resp`. It was shaped as a key→operator dispatch
because free/skip actions are declared as `key=val` strings that the
controller already parses; keeping the raise-for-unknown-condition text
identical preserves the operator-facing error messages when an unrelated
response class faces an HTTP-only condition like `clen`. The
per-condition branching over response attributes (including opposite
`re.M` behavior for HTTP vs base messages) is exactly the knowledge that
used to live on the response classes — it now runs against foreign state
via the `resp` parameter, and the response classes no longer have any
opinion about how their own state is judged.

**Cluster C — controller-side progress accounting.** Progress reporting
after a run (and while watching it) is a controller show, so the counter
logic moved with it. `record_progress` interleaves both state worlds:
worker-owned counters (`hits_count`, `done_count`, `skip_count`,
`fail_count`, `current`, `seconds`) are read and written next to
controller-owned decisions (resume tables, shutdown flag, hit pushing).
`progress_snapshot` and `progress_detail` are the read-only pair the
final report and monitor wait-state share. This cluster was placed on the
controller deliberately: it is the site where a mechanical "hoist the
loop invariant" refactoring is most tempting and where the ownership
question (whose concern is a worker's counter transition?) is least
obvious from reading any single call site.

## Why these sites and shapes were selected

* The three clusters cover both classes of ownership in the run loop —
  the response data owners and the progress data owner — rather than
  three copies of one shape.
* The manifestation differs by role: read-only aggregation, render-only
  formatting, derived calculation, multi-way conditioned branching, and
  mixed read/write lifecycle handling. A single mechanical
  transformation therefore cannot restore locality everywhere at once.
* Variant handling (`isinstance`/`hasattr` probes, HTTP-specific
  conditions) is what makes the response cluster non-trivial: any
  relocation of that knowledge has to respect the response hierarchy,
  since plain, SMB, Oracle and HTTP response objects all flow through the
  same evaluation and reporting paths at runtime.
* The diff keeps all public surface and runtime behavior of the tool
  byte-compatible: same rows, same files, same summaries, same errors —
  only the *placement* of data-oriented behavior changed.
