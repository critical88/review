# Clean up how the reboot lifecycle settings travel through kured's node operations

## The observation

kured's daemon used to keep the whole node reboot lifecycle inline in the
command tier. That logic has since been split out: the drain, uncordon and
post-reboot handling of a node now live in a dedicated node-operations
package, and the command's reboot loop calls into that package as it walks
through the reboot cycle.

The split moved the code but not the data it needs. Every flag-derived
setting of a reboot cycle — where notifications go and which message
templates they use, which labels are applied before and after a reboot, and
how the node drain is executed (delay, grace period, pod selector, deletion
timeout, overall timeout) — is still handed over as individual arguments at
each step, and the reboot loop itself receives the same values one by one
so it can pass them on. Concretely:

- every node lifecycle operation re-declares the subset of those settings
  it happens to consume as its own separate parameters;
- the loop that drives them takes the whole collection so it can forward
  it, and its launch site in the command's `main` enumerates them positionally;
- adding one new drain- or notification-related setting today means editing
  every signature and every call between the command and the node
  operations, in the right positional order, each time.

This is a maintenance problem, not merely a style one: the same group of
config values is being handled as unrelated pieces all along the reboot
control path, so every hop re-encodes knowledge of the group.

## What we want

Rework the wiring between the command tier (`cmd/kured`) and the node
lifecycle operations it calls, so that the recurring settings travel as one
cohesive, well-designed abstraction instead of as separate arguments.

Design notes:

- Give the recurring configuration a single owner type with a clear
  responsibility, and decide consciously what belongs in it. The values say
  different things: some describe where progress notifications go and how
  they read, some describe the pre/post reboot labeling, some describe drain
  execution. Run-scoped identity (the cluster client, the node object, the
  node name) is a different concern from configuration; do not blindly
  bundle everything you find into one catch-all record — decide which parts
  of the recurring group belong together and which travel separately, and
  pass identity per target.
- Thread the abstraction through the node lifecycle operations and the
  command's reboot loop consistently, including the loop's launch in
  `main`. The loop should no longer need to enumerate the lifecycycle
  settings one by one.
- Choose where the new type lives so the dependency direction stays
  natural: the command depends on the node-operations package, not the
  other way around.
- Prefer one shared abstraction with a coherent purpose over several
  per-signature record types that merely re-wrap the same values hop by
  hop; re-wrapping without a shared owner would leave the real maintenance
  problem in place.

## Behavior that must stay exactly as it is

The refactor must be behavior-preserving. In particular:

- Notification timing and selection are load-bearing: the uncordon that
  follows a successful reboot reports through the configured channel with
  the uncordon message template, while the best-effort uncordon performed
  after a failed drain must remain silent — do not unify these two paths
  into one that always (or never) notifies.
- The drain flow must keep its current observable sequence: pre-reboot
  labels first, optional drain delay, the drain notification, cordon, then
  the node drain with the same grace period, pod selector, deletion
  timeout and overall timeout.
- The reboot-time annotations must be published before draining and
  cleaned up as they are today, since other maintenance tooling on the
  cluster observes them.
- All command-line flags, their names, defaults and the
  environment-variable binding in the command tier stay unchanged; the
  package-level flag variables remain the configuration surface.
- The distributed lock usage (pkg/daemonsetlock), the taint handling
  (pkg/taints), the rebooter together with whatever signals a pending
  reboot and any reboot blockers, and the metrics
  endpoint must continue to be used exactly as now; do not redesign those
  packages.
- The existing tests in `cmd/kured` (URL and flag validation, the lock)
  must keep compiling and passing, and the repo's full test suite must
  pass.

## Scope hint

Everything you need is on the reboot control path in the command tier and
in the node lifecycle package it calls; nothing outside that path (lock,
taints, blockers, checkers, time window handling) needs to change. The
cluster-facing API of the node operations is internal to this repository,
so you may adjust their signatures — but keep the daemon's externally
observable behavior identical.
