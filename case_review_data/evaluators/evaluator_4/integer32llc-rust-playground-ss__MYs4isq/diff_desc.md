# Change record: sandbox execution-environment settings

## Maintenance motivation

The playground runs every user request inside a sandbox, and each sandbox only
behaves the way the site promises because the processes inside it agree on some
environmental ground rules: tool output must render identically no matter which
tool produced it (no ANSI escapes, one locale), user panics should be traceable
when the request asked for detail, one request must not monopolize the machine
it shares with others, and every process — the worker itself and the tools it
spawns — must see the same clock and the same sandbox identity. Visitors rely
on these guarantees for consistent output; operators rely on them to keep a
shared sandbox host responsive and deterministic.

Historically these settings were set ad hoc by the code that first needed them.
Over a series of unglamorous tickets ("make rustup output predictable",
"stop the occasional 20-minute build", "make the returned panics match local
ones", "stop tools shipping with the host clock") each rule was introduced by
the maintainer who hit the problem, which is exactly where each of these
additions below has landed. This changeset is the remainder of that work
brought forward so the behavior is explicit instead of implied.

## Normal evolution being modeled

What follows is not one planned refactor; it is the shape the codebase grew
into as separate requirements landed in separate places. Each cluster below was
written by whoever was working in that file at the time and reflects the
natural instinct to put new code next to the code it serves:

- a wire-contract concern ended up on the wire type (`message.rs`);
- a resource-fairness concern ended up in the resource-fairness module
  (`coordinator/limits.rs`);
- a run-detail concern ended up next to the request flag that drives it
  (`coordinator.rs`);
- a sandbox-identity concern ended up in the process spawner that applies it
  (`worker.rs`);
- a process-startup concern ended up in the process entrypoint
  (`bin/worker.rs`).

Each addition is small and locally sensible. What accumulates, as each piece
became a dedicated named function rather than an inline statement, is that the
"what environment does a sandboxed process observe?" question now has its
answer assembled from several independently named places, and the baselines are
merged by hand at each request-lowering site that needs more than one of them.

## Overall design

Five small named functions were added, each owning a handful of environment
name/value pairs for a specific concern. Each lowering site in the coordinator,
the spawner in the worker, and the entrypoint of the worker binary were then
taught to merge whichever fragments apply to what they run. Since the
fragments come in different container shapes (`HashMap<String, String>`,
a shared static slice of pairs, owned `Vec<(String, String)>`), a small
`SpliceEnvironment` glue trait was added in `lib.rs` so every merge site reads
the same regardless of the shape it merges. The five additions, by file:

- `message.rs` — `ExecuteCommandRequest::rendering_contract()` returns
  `&'static [(&'static str, &'static str)]`: the two settings are a property of
  the request that crosses the coordinator/worker boundary, so they are
  declared alongside it.
- `coordinator/limits.rs` — `build_fairness_environment()` returns a
  `HashMap<String, String>`: the job-count cap and the no-network rule are
  resource-fairness policy like the semaphores around them.
- `coordinator.rs` — `run_detail_environment(backtrace)` returns a
  `HashMap<String, String>` containing `RUST_BACKTRACE` when requested: the
  backtrace flag is chosen per request by the coordinator.
- `worker.rs` — `sandbox_floor_environment()` returns
  `Vec<(String, String)>`: the sandbox identity and UTC clock are guarantees
  of the machine the process runs on, applied where processes are started,
  with request-sent keys taking precedence through `entry().or_insert`.
- `bin/worker.rs` — `seed_worker_environment()` writes `TZ` (defaulting to
  UTC when unset) and `PLAYGROUND_WORKER` into the worker's own live
  environment once before listening, so all children inherit them without the
  coordinator sending them with every single request.

The lowering call sites pick which fragments they need by kind: run/build
requests merge all of run detail, fairness, and rendering; format, clippy, and
macro-expansion requests merge only the rendering fragment; miri merges the
rendering fragment into the `MIRIFLAGS`/`MIRI_SYSROOT` map it already built.
The worker floors every spawned process with the sandbox fragment after the
request map arrives.

## Rationale per location

### `compiler/base/orchestrator/src/message.rs`

`ExecuteCommandRequest` is what actually crosses the process boundary and
carries `envs` to the worker, so the output-rendering settings were placed on
that type as an associated function. The rationale when written: the request
carries the environment, so its type is the natural home for the words "this
pair is part of every request's contract". The static-slice shape serves the
UI, which needs to audit the same contract without building a map. Production
role: makes the guarantee inspectable next to the field it describes.

### `compiler/base/orchestrator/src/coordinator/limits.rs`

This module already owns the fairness machinery — the semaphores that cap
concurrent processes and containers. When fairness started being expressed as
environment settings too (a build-jobs cap, forced offline mode), the settings
went here with the rest of the fairness policy. Production role: one place to
see everything a single request is allowed to use.

### `compiler/base/orchestrator/src/coordinator.rs`

The backtrace setting is driven by a per-request user flag, so the fragment
was placed next to the `LowerRequest` impls that make the decision, as a
function over the flag. The five `execute_cargo_request` sites that previously
decided their environment inline were taught to merge `run_detail_environment`
or the fairness and rendering fragments instead, leaving the union entirely to
the merge site. Production role: keeps "what happens for this one request" in
the request layer.

### `compiler/base/orchestrator/src/worker.rs`

The worker is the last component with a say in a process's environment, so
the pieces describing the sandbox itself (identity marker, forced UTC) are
applied after the request map arrives, never overriding what the coordinator
sent. Production role: the guarantees of the machine, not of any one user
request.

### `compiler/base/orchestrator/src/bin/worker.rs`

The worker's own inherited environment is established here before it starts
listening, since some guarantees (timezone, worker marker) should hold for the
worker process and all its descendants rather than travel with each request.
The `TZ` fallback reads the incoming environment before writing, so an
operator-provided timezone still wins. Production role: process startup
behavior.

### `compiler/base/orchestrator/src/lib.rs`

The merge sites accept fragments in three different container shapes, and
without glue each site would need its own matching conversion. The
`SpliceEnvironment` trait gives all of those shapes one spelling
(`fragment.splice_into(&mut envs)`), keeping the
lowering sites uniform. Production role: shared plumbing, deliberately generic
and not dedicated to any one concern.

## Structural variation

The fragments deliberately vary in form rather than all being cut to one
template: two return owned maps built with different idioms, one returns a
`&'static` slice of borrowed pairs, one returns an owned vector of pairs, and
one writes the live process environment directly instead of returning
anything. The merge sites also vary: some compose three sources, the
format/clippy/macro-expansion sites take just the contract, miri keeps its own
map and merges the contract into it, the spawner floors with
`entry().or_insert` (so coordinator-sent keys win), and the entrypoint seeds
the process once. This mirrors how the pieces would realistically have been
added by different passes.

## Behavior notes

Most of these settings are new guarantees rather than relocations: previously
only run/build requests carried `RUST_BACKTRACE` when the request asked for
detail, and every other invocation ran with whatever the sandbox happened to
provide. This changeset makes the rendering contract apply to every tool the
coordinator lowers — including the format, clippy, and macro-expansion paths,
which previously ran with no extra variables — and adds the fairness cap,
sandbox identity, worker marker, and timezone pinning where none existed
before. What is deliberately kept as it was: the existing per-request
`RUST_BACKTRACE` behavior, miri's `MIRIFLAGS` and prebuilt `MIRI_SYSROOT`
approach, the precedence rules (request-sent values win over the worker's
floor; an operator-provided `TZ` still wins), the request/response wire
formats, and the execution behavior of every operation the playground already
supported.
