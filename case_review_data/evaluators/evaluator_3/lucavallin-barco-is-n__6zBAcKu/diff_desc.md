# Injection Design Record — interface_segregation in barco

## 1. Motivation

barco is a small, cleanly separated container runtime. Before the injection, each
subsystem owned a narrow, focused interface:

- `include/container.h` exposed `container_config`, `container_init`, `container_wait`
  and `container_stop`;
- `include/cgroups.h` exposed `cgroups_init` and `cgroups_free`, plus the cgroup
  constants they needed;
- `include/user.h` exposed `user_namespace_init` and `user_namespace_prepare_mappings`;
- `include/mount.h` and `include/sec.h` exposed one function each;
- `src/barco.c` (the orchestrator) called exactly those functions, and each translation
  unit included only the interface of the subsystem it used.

That layout is the textbook counter-image of an interface-segregation problem, which is
precisely why it makes a good host for this smell: the injected state degrades a
crisp per-client contract into one fat contract shared by every client, and every
participant has to accept declarations and operands of that contract that have nothing
to do with its own work.

The target smell is the classic "fat consolidated interface": one integrated
`runtime` interface — an umbrella header holding every subsystem's declarations,
constants and types, a single uniform lifecycle covering every subsystem, and one
shared configuration record handed to every phase — forced onto all subsystems
(implementors) and all translation units (consumers). Subsystems that only have work
in a subset of the phases still have to implement every phase, so most of their
lifecycle becomes no-op stubs. Consumers that need one subsystem symbol still pull in
all of them.

## 2. Evolution modeled

The change is written as a plausible engineering episode rather than a gratuitous
rewrite: a "unify the runtime lifecycle" cleanup that someone could genuinely land.

1. **Centralize configuration.** First the scattered arguments (`container_config` for
   the container, loose `uid`/`fd`/`pid` parameters for the other subsystems) are merged
   into one `runtime_config` record that every subsystem function receives, on the
   theory that a single "runtime state" object is easier to pass around than a growing
   argument list.
2. **Standardize the lifecycle.** Then the ad-hoc function set is normalized into named
   lifecycle phases (`spawn`, `configure`, `activate`, `run`, `join`, `stop`,
   `release`), and each subsystem must implement all of them as a
   `runtime_ops` table of function pointers, on the theory that every subsystem
   becoming interchangeable makes the runtime more uniform.
3. **Consolidate the declarations.** Since the phase table type is shared, all
   subsystem declarations, constants and the configuration type move into a single
   `include/runtime.h` umbrella header. The former per-subsystem headers survive as
   thin re-export shims so existing includes keep compiling — the classic halfway
   state in which nothing is ever cleaned up because everything still works.
4. **Dispatch from the middle.** A new `src/runtime.c` translation unit walks a global
   registry of all subsystem tables, so calling a lifecycle phase means "call the phase
   on every registered subsystem", and `main` in `src/barco.c` talks to this driver
   instead of to the subsystems. Once this stands, every subsystem must depend on the
   directories, limits and namespace constants of the other subsystems, whether it
   uses them or not.

## 3. Design of the injected state

The injected repository adds one consolidated runtime interface:

- **`include/runtime.h` (new, the fat interface)** — every constant that previously
  lived with its own subsystem (`CGROUPS_MEMORY_MAX`, `CGROUPS_CPU_WEIGHT`,
  `CGROUPS_PIDS_MAX`, `CGROUPS_CONTROL_FIELD_SIZE`, `CGROUPS_PROCS`,
  `CONTAINER_STACK_SIZE`, `SEC_SCMP_FAIL`, the `USER_NAMESPACE_*` bit names), the
  shared `runtime_config` record (uid, fd, parent_fd, pid, hostname, cmd, arg, mnt,
  stack_top — reproducing the `container_config` fields), the `runtime_ops` vtable
  type with seven uniform function pointers, an `extern` instance of the table for
  each of the five subsystems, and the prototypes of the runtime lifecycle drivers.
- **Uniform vtable implementors** — `container`, `cgroups`, `mount`, `sec` and
  `user` each define a `*_interface` table of seven handlers. Only the phases that
  correspond to real, pre-existing work contain real bodies; the rest are forced
  no-op stubs (`{ return 0; }`). The mount subsystem owns a single real phase out of
  seven and the security subsystem two; the container subsystem owns five and also
  has to carry a `stop` phase that no caller ever dispatches.
- **`src/runtime.c` (new, the registry dispatcher)** — holds the
  `runtime_registry` array of all subsystem tables and implements the seven
  `runtime_*` drivers as loops over it, returning the first failure like the
  original `||` chains did.
- **Shared-config phase signatures** — every handler takes
  `runtime_config *config` so that it fits the single `runtime_ops` pointer type,
  even where the original function took a loose parameter or none at all (for
  example the capability/seccomp phases, which take all their inputs from globals
  and now ignore the operand entirely). `container_start(void *arg)`, the
  `clone()` trampoline that must keep its generic prototype, now has to cast to the
  shared record like its original `container_config` sibling did.
- **Compatibility shims** — `include/cgroups.h`, `include/mount.h`,
  `include/sec.h` and `include/user.h` keep their historic guard macros but now do
  nothing except `#include "runtime.h"`; `include/container.h` keeps its original
  content and adds the runtime import. Every subsystem translation unit includes the
  old focused name of its own interface, but the declarations and constants it
  consumes actually come from the umbrella and from each other's former sections.
- **Orchestrator coupling** — `main()` in `src/barco.c` constructs the single
  `runtime_config` record, hands its address to every dispatcher call, and receives
  all configuration work through the parent→child socket pair exactly as before;
  the individual subsystem interfaces it consumed are gone from its usage, only
  their shim includes remain. The `Makefile` object list gains `runtime.o` through
  the shared `$` variable family, so the flow touches the build shape too.

## 4. Per-location rationale

| Location | Role in the injection | Rationale |
| --- | --- | --- |
| `include/runtime.h` | The consolidated interface: all constants, all types, all subsystem tables, and the phase prototypes in one file. | The fat interface itself. Every constant of every subsystem — cgroup limits, capability bits, stack size — now reaches every participant through the same header, while also declaring the full standard lifecycle that each implementor must satisfy. |
| `include/cgroups.h` | Shim: guard macros plus `#include "runtime.h"`. | Preserves the halfway state of the modeled evolution: includers believe they consume a per-subsystem interface, but the vendor is now the single umbrella, so the cgroup limits leak to namespace and mount code. |
| `include/container.h` | Keeps its own declarations, adds the runtime import. | `container_config` is superseded by the shared record, so this was already a good consolidation lever; leaving it as the one non-shim focused header shows the interface is "mostly" consolidated, like a real tree. |
| `include/mount.h` | Shim | Same as cgroups. The mount subsystem is the clearest implementor-side victim — one real phase out of seven, so it must carry six no-op members. |
| `include/sec.h` | Shim | The capability/seccomp phases need no runtime configuration at all, which makes the forced shared operand maximally independent of real work. |
| `include/user.h` | Shim | User-namespace code used to take loose `(uid, fd)`-style parameters and now receives the full shared record whether or not the fields concern it. |
| `src/runtime.c` | Registry of every subsystem table and seven phase loops. | The dispatch middle: once a phase is driven through a global registry, `run` on one subsystem is coupled to `release` on all subsystems, and the per-subsystem lifecycle interfaces have no independent existence left. |
| `src/container.c` | Implements all seven container phases (clone is real; `configure`/`release` are forced no-ops; `stop` exists only for the phase contract) and the `container_start` trampoline driving the dispatcher from inside the child. | The container subsystem is the lifecycle anchor and stays mostly real, but still has to accept the uniform operand in every phase; its never-dispatched `stop` phase makes the "implement everything" pressure explicit despite no runtime driver calling it. Having the child call the phase driver instead of calling the subsystems directly is what makes `runtime.c` capable of dispatching every phase in order, and is the price of not reworking parallel dispatch logic on each side of the `clone()`. |
| `src/cgroups.c` | Two real phases extracted from the old `cgroups_init`/`cgroups_free` plus four forced no-ops. | Cgroups work follows one phase (`configure`) and one for cleanup; everything else is the textbook content-free stub of a fat interface. |
| `src/mount.c` | One real phase (`activate`) out of seven. | A subsystem whose knuckles need one syscall phase; its six no-op phases carry no possible content and can only be padding. |
| `src/sec.c` | Keeps its two static helpers, forces them behind one real phase (`activate`) that ignores the shared operand, plus five no-ops. | Features the strongest case of an implementor forced to accept a parameter it never touches. |
| `src/user.c` | Two real phases (`configure` for uid_map/gid_map writing, `activate` for the unshare handshake) plus five no-ops. | The principal actual consumer of the shared record, so its now-unused parameters genuinely come from the consolidation and not from leftover debugging. |
| `src/barco.c` | Replaces per-subsystem calls with `runtime_config` construction plus uniform phase calls through the dispatcher. | The consumer side: the orchestrator stops depending on subsystem-specific interfaces but still has to build and pass the one shared record, blurring resource-specific setup (cgroup limits, mount dirs, capability bits) into one fused call surface. |
| `Makefile` | Adds `runtime.o` to the object list. | The new dispatcher translation unit must be linked; keeping the affected surface to the object-list line keeps the change minimal. |

## 5. Behavior deliberately preserved

The injection was written against an explicit preservation contract, and the design
chooses one-phase-per-loop dispatch specifically so that behavior does not have to
change:

- the sequence of `clone` → write cgroup limits and `cgroup.procs` → write
  `uid_map`/`gid_map` → child `sethostname` → bind mount/pivot_root → `unshare` +
  socket handshake → capability drop → seccomp load → `execve` → parent `waitpid` →
  cleanup/teardown is identical before and after the diff;
- each registry-driven phase is a first-failure loop, matching the short-circuit
  semantics of the original `||` chains, so the first failing subsystem still aborts
  the phase;
- the historical quirk that a failed container creation returns `1` while `main`
  compares against `-1` is kept verbatim, and the child-side socket/passing fd
  protocol of the parent↔child handshake, the stack buffer ownership and the cgroup
  path naming are untouched;
- the log narration keeps its original step labels (`"initializing cgroups..."`,
  `"configuring user namespace..."`, `"waiting for container to exit..."`,
  `"freeing resources..."`). Two purely narrative drifts remain on failure paths,
  the kind an ordinary refactor produces: `main` no longer knows which subsystem
  failed during configuration, so the two per-subsystem fatal messages it printed
  on limit/mapping failure collapse into one phase-level message, and the dispatcher
  contributes one `log_debug` line per phase before driving the registry;
- `make` and `make test` keep succeeding on the injected tree, `--help` and the
  argument-error path print the same strings (modulo source line numbers embedded by
  the logger), and a container launch attempt performs the same steps with the same
  exit code.

## 6. Scope decisions

Additional candidates were considered and rejected for the diff:

- third-party code (`lib/argtable`, `lib/log`) and the argument-parsing model is left
  untouched — vending an unrelated CLI model into the runtime interface would inject
  a second, unrelated smell;
- `pivot_root()` in `src/mount.c` stays a private helper of the mount subsystem;
  promoting it into the interface would only restate the umbrella pattern inside one
  subsystem, adding no new role;
- `tests/barco_test.c` is not modified; the CUnit suite contains no project-header
  usage, so editing it would produce harness noise instead of smell surface;
- no further lifecycle phases or registrations were added past the uniform seven-phase
  lifecycle: the runtime has no additional distinct moments to force onto
  subsystems, so an eighth phase (or a sixth registered subsystem) would only
  reproduce the same content-free stub role at the cost of padding, and the five
  real subsystems are the only genuine production owners of container runtime work.
