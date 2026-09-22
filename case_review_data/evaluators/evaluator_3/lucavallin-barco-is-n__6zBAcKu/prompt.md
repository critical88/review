# Please straighten out the runtime lifecycle interfaces

## Background

Our container runtime grew out of a "unify the lifecycle" cleanup a while back. Before
that, each subsystem had a small header of its own and you could see at a glance what
a piece of code was allowed to call. Now there is one shared interface that holds
everything, and every subsystem is expected to fit the same lifecycle shape.

The code builds and the container still runs, so nobody has touched it since. But we
keep tripping over the consequences:

- A subsystem whose entire job is one step still has to provide a handler for every
  phase of the lifecycle. Most of those handlers are empty `return 0` bodies that
  exist only to fill a slot, and nobody can tell at a glance which phases are real.
- Every handler takes one shared configuration record as its parameter, so a phase
  that reads nothing from it still receives all of it, and the signature tells you
  nothing about what the phase actually uses.
- The old per-subsystem headers are still there but no longer declare anything of
  their own, so figuring out where a constant, type, or function really comes from —
  or which subsystem genuinely owns a given step — means reading the whole interface
  rather than one focused file.
- Threads of reasoning that used to be local to a subsystem (which mount setup
  happens where, whether resource limits are applied by the parent or the child)
  now cut across every module, and adding one real phase to one subsystem drags in
  the whole shape of the interface.

## What we would like

Please give the runtime back its per-subsystem boundaries: each subsystem owning the
operations it actually performs, and each translation unit depending only on the
interface of the subsystem it uses, instead of one consolidated surface forcing every
module to fit the same shape.

We deliberately do not have a preferred layout in mind — reintroduced focused
headers, per-subsystem operation sets sized to their real work, direct includes per
consumer, and whatever becomes of the intermediate shared machinery are all your call.
Any design that removes the "everyone implements everything, everyone includes
everything" shape is fine. The slimmed-down/dead pass-through headers, the
filler phases, and the "one record for everyone" signatures are all fair game for
rethinking — they are scaffolding from the consolidation, not features.

## Scope

The runtime sources under the project's `src/` and `include/` directories. The vendored
third-party code under `lib/`, and the CUnit test suite, should not change; this is a
boundary cleanup, not a rewrite of functionality.

## Must remain true

- `make` then `make test` must keep working as the project's build and test workflow,
  both succeeding.
- Runtime behavior is frozen: the identical lifecycle syscall order, the
  short-circuit-on-first-failure semantics for each phase, the parent↔child socket
  protocol, cgroup limits and paths, uid/gid mapping ranges, dropped capabilities,
  the seccomp rule set, stack sizing, and the observable `barco`/`barco --help`
  command-line interface.
- Preserve the existing control flow and failure handling exactly as it is —
  including its current quirks; do not turn a failed setup into a silent
  continuation or add new fallback behavior.
- The source keeps building cleanly under the project's standard C toolchain settings,
  with no new third-party dependencies.
