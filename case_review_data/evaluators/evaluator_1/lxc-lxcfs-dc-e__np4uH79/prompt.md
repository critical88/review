# Refactor task: consolidate the cgroup memory/swap budget threading in the procfs reporting subsystem

## What we observed

`lxcfs` makes containers see their own memory and swap budget by rewriting
`/proc/meminfo` and `/proc/swaps`. A recent cleanup consolidated the
cgroup-side resolution of that budget into a shared helper in the cgroup
utility layer and deduplicated the swap-total clamp policy that both report
readers had carried as hand-written copies. The cleanup removed real
duplication, but it left the budget's values handed through the reporting
code one scalar at a time:

- the shared resolution helper returns each budget component (the effective
  RAM limit, the RAM usage, the memory+swap limit, the memory+swap usage,
  and whether swap accounting is resolvable at all) through its own
  out-parameter;
- the helper that turns that budget into derived swap figures takes the
  same values as a long positional parameter list and writes its three
  results through more individual out-pointers;
- the shared clamp helper is threaded the same way;
- and both report readers unpack and pass those scalars along the chain
  positionally.

All of these sit in two places: the cgroup utility layer that resolves
controller values for reporting, and the procfs virtualization layer that
renders the two reports above. Anyone adding, reordering or renaming a budget
component now has to touch every signature, declaration and call site along
the chain — and since the parameters are adjacent values of the same type,
nothing catches passing them in the wrong order: it compiles and quietly
reports the wrong number.

## What we want

Investigate the memory/swap budget flow that feeds container-visible memory
reporting — from the cgroup utility layer's resolution of the effective
limits, usages and swap-account availability, through the swap figures
derived from them, the clamp policy shared by both readers, and the two
report entry points — and restructure it so the values that form one budget,
and the swap report derived from it, are each carried by one well-designed
record type end to end instead of long same-type parameter lists.

This is not a single-location fix: apply the restructuring at every
function signature, declaration, and call site that participates in the
positional threading you find in that flow. The consolidation itself is
wanted and should stay: keep one shared resolution point in the cgroup
utility layer and keep the clamp policy in one place; only the way the data
travels needs to change.

## Behavior and compatibility constraints

- The user-visible output of `/proc/meminfo` and `/proc/swaps` must remain
  identical in every configuration: cgroup v1 and cgroup v2 hosts, swap
  virtualization enabled and disabled by mount option, kernels with swap
  accounting on and off, cgroups with and without limits, and the
  MemTotal/SwapTotal/SwapFree rules including the host swap-device clamp
  and the zero-swappiness suppression.
- The resolution semantics must not change: which memory controller values
  are consulted and in which order, which failures fall back to
  host-complete output versus an empty report, which failures are tolerated
  by reporting a zero swap budget with the default swappiness hint, and the
  point at which byte values are converted to kB in the meminfo path.
- The FUSE operations and every pre-existing public function of the
  library and its installed headers keep their signatures; headers may, of
  course, define new record types for the refactored flow.
- The build and the repository's test suite must stay green.

## Suggested first steps

- Start from the two `/proc` report readers in the procfs virtualization
  layer, then walk up through the derived swap figures and the clamp policy
  to the shared helper in the cgroup utility layer that resolves the
  budget, noting every signature that passes pieces of that budget
  positionally.
- Re-check the whole reporting subsystem rather than the most obvious
  signature, including any declaration that repeats the same parameter
  list for API readers.
