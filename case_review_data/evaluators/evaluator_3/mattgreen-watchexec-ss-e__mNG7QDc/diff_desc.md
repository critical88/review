# Injection design record — watchexec signal-stop behaviour work

## Context and maintenance motivation

watchexec starts a command, watches filesystem events, and restarts the command when they
change. A small but behaviour-critical aspect of the tool is *what happens when a command
has to be stopped*:

- which concrete signal is used to stop (or to signal) the running command, resolved from
  the `--signal` and `--stop-signal` options, with a built-in fallback when neither is
  given (`Signal::Terminate`);
- which signals received by watchexec *itself* should be treated as an immediate "stop
  watching" request rather than as an ordinary event.

The `--help` text is precise about the interaction between the two options:
`--stop-signal` is documented as the signal used "when it's still running" by the
`signal` mode of `--on-busy-update` *unless* `--signal` is provided, and `--signal` is
documented as the signal sent "when it's still running". So the two options are two
*offerings* with a precedence that the documentation spells out once, and every place
that acts on them has to honour that precedence.

At the pinned commit, the CLI reconciles these options once while assembling its
configuration: the timeout area keeps a single already-resolved stop signal value, the
interactive stop/restart branches each apply the resolved value, and one helper
(`on_busy_signal`) encodes the "unless `--signal` is provided" twist for busy
signalling. The received-signal side is terse too: emit-only mode and the unmapped-signal
quit check state the interrupt/terminate pair inline as ordinary boolean conditions, and
the runtime's signal event source decides event priority with the pair inside the
expression that is passed to the event channel.

## The normal development evolution being modelled

The change models a believable maintenance arc that any maintainer of this code could
have produced while chasing two very ordinary tickets:

1. *Timeout behaviour alignment.* The `--timeout` feature stops the command after a while.
   A maintainer notices the timeout paths stop with a value that was resolved far from
   the point of use, at configuration-assembly time, and wants the timeout paths -- the
   once-mode handler **and** the little watcher process spawned for stdin-redirected
   runs -- to see the *offerings as given*, so that what a timeout stop uses is decided
   where it happens, visible in isolation, next to the code that acts on it. To do that
   the timeout configuration struct has to carry the options as given instead of a single
   pre-resolved value.

2. *Terminal-grade signals are special.* A second, unrelated-seeming piece of work makes
   the interrupt/terminate pair an explicit, locally-visible decision at each point it
   matters: watchexec should quit promptly in emit-only mode when the user hits
   Ctrl-C or the system sends a terminate; it should quit when the received signal is not
   mapped by `--map-signal`; and the runtime should pre-empt the queue for such signals
   (they "nearly always mean stop now", so they should skip the debounce). The maintainer
   writes that pair-membership decision out in full at each place rather than expressing
   it as a terse inline condition, so each consumer reads standalone.

The two strands mix, as they do in real histories: the same functions grow local
"resolve among the offerings" tables and local "is this a stop request" tables, in two
crates (the CLI and the runtime library). Each addition is defensible in a review; the
combined shape is what this record documents.

## Overall design

Two production files change. The first, the CLI's configuration/action-handler assembly:

- The timeout area's configuration struct now carries the `--signal` and `--stop-signal`
  options *as given* (two optional values with doc comments quoting what each option
  means) instead of a single resolved signal.
- Every branch that stops or gracefully quits the watched command -- the Ctrl-C graceful
  quit, the once-mode timeout stop, the interactive stop key, the interactive restart
  key, and the crash-restart branch of `--on-busy-update` -- resolves which signal to use
  on the spot, with a small `match` over the option pair.
- The timeout watcher helper spawned as its own process resolves the same pair
  independently from its own copy of the configuration.
- The emit-only quit check and the unmapped-signal quit check spell out the
  interrupt/terminate pair as loop-and-match tables setting a flag.

The second, the runtime library's signal event source: the priority of an emitted signal
event is hoisted out of the `send` call into a local decision made up front, classifying
the interrupt/terminate pair.

Comment style follows the file's habits: lower-case, casual, quoting the documentation
for the two options ("`--signal` is for signalling a busy command, not for stopping", "the
stop offering decides, with the built-in last resort"). Tabs, line width, and control-flow
shape (early returns, `cfg!(windows)` splits around every stop call) mirror the
surrounding code. No public API changes; the windows branches retain their
platform-specific escape hatches.

## Per-cluster rationale

### Cluster A — the timeout configuration carries the offerings as given

`TimeoutConfig` (in the CLI's config module) previously stored one already-resolved stop
signal for the timeout paths. It now stores the pair of optional offerings. Cluster
reason: the once-mode handler and the spawned watcher process are two separate consumers
that both need the pair at their point of use; carrying the pair (rather than each
consumer re-fetching its own arguments) is the smallest change that keeps both
consumers' decisions local. Production role: transport of the user's intent into the
spawned process.

### Cluster B — stop-time resolutions go local in the action handler

Five branches in the CLI's action handling now fit the same shape: when the watched
command must be stopped or signalled to stop, a two-line `match` over
`(signal, stop_signal)` picks the stop offering, with the built-in fallback. Locations:

1. **Ctrl-C graceful quit** (the interactive loop's "exit now" path): decided inline in
   the `quit_gracefully` call.
2. **Once-mode timeout stop**: hoisted into a local binding just before
   the stop call, reading the pair from the timeout configuration.
3. **Interactive stop key**: inline in the stop-with-signal call.
4. **Interactive restart key**: inline in the restart-with-signal call.
5. **`--on-busy-update` restart while running**: inline in the restart-with-signal call.

Why this shape was selected: the maintainer wanted each branch to read standalone.
Two-line matches over the option pair are the house style already present in the
busy-signalling helper, so extending that idiom to the new decision points is
consistent, reviewable line-by-line, and avoids inventing a shared helper mid-feature.
The comment on each table notes what the offerings mean for that branch (the busy
signalling helper is the only case where `--signal` wins, per the documentation), which
is exactly the kind of comment watchexec already carries next to its option handling.
Production role: each of these branches is a distinct lifecycle decision over the watched
command, each in its own sub-handling path (quit vs timeout vs stop vs restart vs
crash-restart).

### Cluster C — received-signal stop-request checks spelled out

Two checks in the CLI's action handling restate the "did we get told to stop watching"
pair membership in full:

1. **Emit-only mode** (no command running): a loop over the action's received signals
   fills a flag when the local table classifies the signal as interrupt/terminate. The
   terse iterator-based condition that was there before is judged sufficient for the
   author who wants the pair visible where it decides, and loops are already used
   elsewhere in the same handler.
2. **Unmapped-signal quit** (normal mode): a loop over the received signals classifies
   the same pair, with an arm guard consulting the `--map-signal` map, because the
   decision here has an extra "as long as the user didn't map it themselves" wrinkle that
   reads more clearly as a table arm than as a compound boolean.

Cluster rationale: both sites are quit decisions for the *tool itself* rather than the
watched command, so the pair-membership rule appears with different trimmings
(immediate quit vs mapping check). Production role: exit-behaviour decisions in the two
operating modes of watchexec.

### Cluster D — the spawned timeout watcher re-derives its stop signal

The watcher helper for `--timeout` in stdin-redirected runs runs as its own process. It
receives the timeout configuration (cluster A's pair) and, when the timeout fires,
resolves the stop signal from its own copy with the same two-line table. Why this site:
the watcher is intentionally a self-contained little program (it must babysit the job
while the main process is otherwise engaged); its maintainer prefers it not to depend on
resolution done elsewhere in the main process. Production role: an independent process
deciding the same lifecycle question at its own point of use.

### Cluster E — runtime priority escalation made explicit

In the library's signal event source, the priority assigned to an emitted signal event
moves out of the `send(...)` argument and into a local binding computed up front, with
the interrupt/terminate pair classified by a short table. Why this shape: the event
channel argument was already hard to read; hoisting the priority decision gives it a
name and a place to breathe, and the neighbourhood (a platform-dispatching worker above)
makes table-shaped decisions locally idiomatic. Production role: queue-pre-emption —
signals that mean "stop now" skip the debounce and land at the head of the queue, which
matters when the user is hammering Ctrl-C while a long debounced run is queued.

### Not touched (and why)

The busy-signalling helper's own table keeps its documented twist (`--signal` wins there)
— it is the origin of the idiom the evolution extends. The supervisor's job-side
delivery keeps receiving the decided signal as an argument; it is a delivery mechanism.
The shared `Signal` type is not extended: the evolution modelled here spends its effort
in the consumers rather than in the type. CLI argument parsing, the shell and
engine configuration, and all tests are unchanged.
