# Consolidate the stop-policy decisions around signals

## Maintainer observation

While preparing a small behavioural change for the next release — letting the built-in
fallback signal for stopping a watched command be revisited without an archaeology dig —
I traced where watchexec decides signal-stop behaviour, and the answer is scattered.
The `--help` text states the semantics precisely once: `--stop-signal` names the signal
used to stop the command, `--signal` names the signal sent to the command "when it's
still running" (busy signalling), and the busy-signalling mode of `--on-busy-update`
uses `--stop-signal` "unless `--signal` is provided". But in the code, two distinct
policy facts are re-derived locally wherever they matter instead of being owned once:

1. **Which signal do we stop (or signal) the command with?** The `--signal`/`--stop-signal`
   option pair is reconciled on the spot wherever a stop is decided. Graceful quit, the
   timeout paths, the interactive keys, `--on-busy-update`'s branch handling, and the
   little watcher process the timeout feature spawns for stdin-redirected runs are
   between them enough copies that I stopped counting; some read the options directly,
   others get them carried through a config struct, and the watcher resolves them over
   again in its own process. The busy-signalling helper that already exists encodes the
   documented precedence twist (`--signal` wins there) in still another local shape.

2. **Which received signals mean "stop watching now"?** The interrupt/terminate pair is
   re-listed wherever a received signal is interpreted on the tool side — the quit
   decisions in the CLI's action handling are the obvious ones — and the runtime's
   signal event source also has its own copy of the pair for the priority it assigns,
   which is what makes Ctrl-C pre-empt the queue.

A conceptual change to either fact — a different built-in fallback than `Terminate`,
honouring `--signal` for stops after all, or extending the "stop request" pair to another
signal — currently means coordinated small edits at every decision point, in both the
CLI's configuration/action-handler assembly and the runtime's signal source. That is the
shotgun-surgery shape, and it is where most of the maintenance risk in this area now
lives.

## Desired outcome

Restructure so each of the two facts is decided in exactly one place, and every consumer
asks that place instead of re-deriving the answer. Follow the codebase's own precedent
for where such knowledge lives: a fact that is about signals themselves — like which
signals mean "stop" — fits naturally as a method on the shared `Signal` type in the
`crates/signals` workspace crate (which the CLI and the runtime both already depend on),
while a decision that weighs the options against the situation — like which offering a
stop uses — belongs in one function with that single responsibility. Concretely, after
the change:

- locating where "which signal stops the command" is decided takes you to one place, no
  matter which stopping path asked (graceful quit, timeout, interactive key, or
  busy-update), including from inside the timeout watcher process;
- the interrupt/terminate "stop request" membership is stated once and consumed by both
  the CLI's quit decisions and the runtime's priority decision;
- nothing re-derives the same answers you will consolidate — whatever copies I managed
  to spot while tracing should all be replaced by asking the one place each;
- our existing unit tests for the `--signal`/`--stop-signal` precedence are adapted to
  the new structure and still pin down the documented semantics, including that
  `--signal` wins only for busy signalling, and that stopping ignores it.

This is a refactoring, not a feature: keep the observable behaviour bit-for-bit. Scope is
the two facts above wherever they are decided today — the CLI's action/config assembly
and the runtime's signal handling are both in scope; CLI argument parsing, the
supervisor's process-level delivery, and the `Signal` type's presentation/serialisation
machinery are out of scope beyond what the consolidation needs.

## Behavioural requirements

These must be preserved exactly as they stand today:

- When both options are given, stops use `--stop-signal` (including timeout stops, the
  interactive stop/restart keys, graceful quit, and the spawned watcher's timeout stop);
  `--signal` is used only for busy signalling in `--on-busy-update`'s signal mode.
- `--signal` alone does **not** decide what a stop uses; a stop with only `--signal`
  given behaves as if nothing was given.
- When neither option is given, the built-in fallback is `Signal::Terminate`.
- Receiving interrupt or terminate quits watchexec in emit-only mode, and in normal mode
  quits when the signal is not remapped by `--map-signal`.
- Interrupt and terminate keep pre-empting the event queue (the runtime's urgent
  priority path), and interrupt keeps its keyboard event-source tag.
- The platform escape hatches (`cfg!(windows)` branches around stop calls) keep their
  current semantics; do not unify them away.
- No CLI surface changes: argument parsing, `--help` text, and the config-file surface
  stay untouched.

## Compatibility boundary and acceptance

- Public API stays compatible within the workspace: the CLI binary's interface and the
  published crates' existing items are unchanged. Adding methods to the workspace-internal
  `Signal` type (or a shared helper) is fine and expected; do not add new external
  dependencies or crates.
- The `Signal` enum is `#[non_exhaustive]` and `Copy`; anything added to it should respect
  that (e.g. const-callable, cheap).
- `cargo build --locked --workspace` and `cargo test --locked --workspace --no-fail-fast`
  must both succeed, and the precedence tests must keep pinning equivalent semantics —
  they report the option interaction as documented today.
- Comments that merely narrate a local decision (as opposed to explaining a policy)
  should not survive the restructure.
