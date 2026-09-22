# Injection design record — reboot lifecycle split without the data moving with it

## Maintenance motivation

kured's daemon had grown to the point where the entire node reboot
lifecycle — labeling, notifying, draining, uncordoning — lived inline in
the single command file, next to flag wiring and the metrics endpoint. A
split like that usually starts with the best intentions: the reboot control
code is the part operators have to reason about during incidents, and it
is also the part that clients of the project most often ask to reason about
separately. An extraction of the node lifecycle operations into a dedicated
package is the kind of refactoring kured's own history is full of
(pkg/taints, pkg/timewindow and pkg/blockers were all carved out of the
command the same way), so this change models that familiar next step: the
node lifecycle operations finally leave the command tier and get a package
of their own.

The modeling decision that carries most of the weight here is time-travel
honesty: the split moves the *code* but does the *data plumbing* the quick
way, which is by far the most common first pass. When you extract a
function from a big file, the fastest correct move is to pass everything
that the extracted body reads as explicit parameters. Flag-derived values
that were package-level globals in the command are suddenly parameters,
and each extracted operation declares precisely the slice of them it
consumes. Without a deliberate second pass, the same handful of reboot
cycle values now marches through every signature between the command and
the new package — the classic outcome of a mechanical deglobalization.

## The normal evolution being modeled

The trajectory is the common lifecycle of an extraction:

1. The lifecycle bodies outgrow the command file, so they move to their
   own package, taking their package-private helpers with them.
2. Because the moved bodies previously read the command's flag variables
   directly, the extraction converts those implicit inputs into explicit
   parameters — long, positional, per-operation.
3. The loop that orchestrates the cycle now has to *receive* everything too,
   because it is the one hand-off point between flag-land and the node
   operations package: its own signature swells to cover the union of what
   the lifecycle steps need, and its launch site in `main` becomes a long
   positional call.
4. The TODO to "group these settings into a type someday" is left for the
   next rainy day — which is where the repository now sits.

## Overall design

Three things changed in the tree:

- **A new node-operations package** holds cordon/drain, uncordon, and the
  pre/post-reboot label and annotation maintenance, all as exported calls
  on a `*kubernetes.Clientset`. It is one more peer of pkg/taints and
  pkg/daemonsetlock: small, cluster-facing, and reusable by the daemon
  command (and, in time, by other commands and integration contexts).
- **The command file shrinks** to wiring + control flow: flag parsing,
  client construction, lock construction, the reboot loop, and the
  metrics endpoint. The loop is the only orchestrator of the lifecycle
  and is where the per-cycle values converge before being handed down.
- **The loop's signature and its launch call grew** and the node
  operations each re-declare the subset of cycle values they consume,
  which is what the whole case is about.

## Per-cluster walkthrough

### The node lifecycle operations (new package, node lifecycle file)

`Drain`, `Uncordon` and `UncordonAfterReboot` were lifted nearly verbatim
out of the command, which is why the shape feels so faithful to the
original: their bodies, log lines, error handling and the kubectl drain
helper configuration are the production code the daemon already ran, only
re-homed. Each declares exactly what it consumes:

- `Drain` covers the pre-reboot half of the cycle: the pre-reboot labels,
  the drain delay, grace period, pod selector, deletion timeout and overall
  timeout, plus the notification URL and the drain message template used
  to announce the drain.
- `Uncordon` covers the plain uncordon and post-reboot re-labeling.
- `UncordonAfterReboot` is the recovery path the loop takes when it
  finds itself holding the lock after a successful reboot: it uncordons
  and then reports the node back using the uncordon message template. It
  exists separately from the plain `Uncordon` because only the
  successful-reboot path notifies — a detail operators rely on, and the
  reason the two operations must not be collapsed into one always-notify
  variant during future maintenance.

Why this shape: per-value threading is what a first-pass extraction
naturally produces, and it keeps the moved bodies behaviorally identical
(the same values in the same order into the same statements) — the safest
possible move-extraction, which is exactly why people ship it in that
shape and why the recurring group then digs in.

### Label and annotation maintenance (new package, labels file)

`AddNodeAnnotations`, `DeleteNodeAnnotation` and the package-private
`updateNodeLabels` moved with the lifecycle code, but interestingly they
did *not* take flag values with them: they take a client, a node name (or
node object) and the annotation/label payload. These are internal
maintenance helpers of the new package — they patch node metadata during
the reboot transition — and they participate in the overall narrative of
the command shedding its lifecycle responsibilities without fully
shedding the data-plumbing workload. They each take only two to three
values and are naturally cohesive, so they read as ordinary helpers, not
as part of the settings problem.

### The command's reboot loop (command file)

The loop function is the daemon's scheduler-critical heart and the only
caller of the node lifecycle operations, so after the extraction it is the
convergence point for every per-cycle value: it still fetches the node
object per tick, holds and releases the distributed lock, consults the
time window and the reboot signal, decides annotation publication, and
now also carries the settings the lifecycle operations need. That is why
its parameter list spans both run-control values (the tick period, the
reboot delay, the force-reboot and annotation flags, the taint name to
apply) and the full per-cycle configuration that it forwards to drain and
uncordon. The `node *v1.Node` the loop fetches and the node name it was
launched with travel side by side, which keeps the identity of the target
ambiguous at a glance: one is an API object, the other a string, and both
are "the node".

The loop-top recovery branch keeps its original sequencing: uncordon if
the node is still marked unschedulable, annotate cleanup once no reboot
is pending, release the lock. The drain-failure branch keeps its
force-reboot/best-effort split, and the best-effort uncordon deliberately
does not announce anything — that asymmetry was load-bearing before the
split and remains so after it.

### The launch site (command file, `main`)

`main` is the assembler: it binds flags, builds the client, the lock, the
rebooter and the window, and launches the loop goroutine. Because the
loop now needs everything, the goroutine launch grew a long positional
argument list — the package globals it used to reference are now
arguments because the extraction made the loop's inputs explicit. This is
the third place the same values get enumerated, which is what turns "some
long signatures" into a wiring problem that spans the daemon from its
entry point to its cluster calls.

## Production roles, one paragraph each

- **Node lifecycle package**: the cluster-facing behavior of the daemon
  for node state transitions; owned by the maintenance story above, calls
  into the k8s clientset, kubectl's drain helper and the notification
  sender.
- **Command wiring**: parsing and validation of the operator contract —
  flags, environment, URLs, alert filters, the metrics endpoint. Also the
  composition root that constructs the distributed lock, the reboot
  strategy and all collaborators by configuration.
- **The reboot loop**: the state machine that, per tick, decides whether
  a reboot is needed, is allowed now, and can be attempted safely; it owns
  the lock lifecycle and the annotation protocol.
- **Settings values**: the operator-facing personality of the daemon —
  where it reports, how it reads, how it labels, how it drains.

## Why the sites and shapes were selected

The node lifecycle is where the values have the strongest shared meaning:
notification identity, drain behavior and the pre/post labeling all belong
to "one reboot cycle of one node", and they recur at three granularity
levels (whole-cycle, per-operation subset, per-call threading). Putting
the recurrence in the reboot control path rather than in, say, the flag
parsing or the lock package keeps it on the code that the daemon's
operators actually read during on-call incidents, which is also where a
maintainer would realistically first complain about the argument lists.
The extraction being new-but-plausible means the case looks like the
tedium phase of real growth: everything builds, everything works, nobody
wants to touch it again.
