# Injection design record — lxcfs memory-budget plumbing refactor

## Maintenance motivation

lxcfs rewrites `/proc/meminfo` and `/proc/swaps` for containers so that a
container sees the memory and swap budget of its own cgroup rather than the
host's. Both readers are driven by the same underlying data — the effective
RAM limit, the RAM usage, the memory+swap limit, the memory+swap usage, and
whether the memory controller exposes swap accounting at all — but before this
change each reader resolved that data by itself, with hand-written fetches
interleaved with report formatting.

The realistic maintenance problem being modeled is the one an lxcfs maintainer
would hit next: the two report paths drift. The resolution sequence, the
"swapaccount may be off" tolerance, and the v1-vs-v2 swap arithmetic had to be
kept in sync by hand in two places, and any new budget field (zswap reporting,
memory stats keys, cgroup v2 swap devices) would have to be threaded through
both readers separately.

## Normal development evolution being modeled

The change is a routine "hoist the repeated cgroup fetches into a shared
helper" refactor, the kind that grows during a feature cycle rather than being
designed up front:

1. A shared resolver is introduced in the cgroup utility layer so both report
   paths resolve the memory/swap budget from one place; the low-level limit
   readers move with it, closer to the hierarchy-walking machinery they sit on
   top of.
2. The swap-info helper is rewired to consume the resolver's results, so its
   signature grows to receive each budget value the derived swap numbers
   depend on.
3. The swap-total clamping policy, previously duplicated verbatim in both
   readers, is extracted into one shared helper so there is exactly one copy;
   that helper's signature likewise grows to receive each value it clamps.

As this evolution proceeds the natural (and common) outcome is that the same
grouping of values is drilled through every function on the path one parameter
at a time, rather than being introduced as one bundled concept. The diff
records exactly that state: the reporting flow is consolidated and
deduplicated, while the values it works on are handed around positionally.

## Overall design

* **`src/cgroups/cgroup_utils.c` / `src/cgroups/cgroup_utils.h`** — the
  cgroup utility module, which already owns the hierarchy-walking helpers the
  memory-controller reads depend on, gains the low-level limit readers and a
  new exported budget resolver next to them.
* **`src/proc_fuse.c`** — the procfs virtualization layer: the swap-info
  helper and both `/proc` report readers shrink to consume the shared
  resolution, and the duplicated clamp policy becomes a single helper.

Two properties of the repo guided the shape: the procfs layer and the cgroup
utility layer already cross-call (`proc_fuse.c` includes the cgroup utility
header for walk helpers), and the memory controller exposes each budget value
through a separate driver operation (`get_memory_max`,
`get_memory_current`, `get_memory_swap_max`,
`get_memory_swap_current`, `get_memory_swappiness`), so a shared resolution
point has a natural 1:1 mapping onto controller reads.

## Changed locations

### 1. Low-level limit readers moved into the cgroup utility layer

`src/cgroups/cgroup_utils.c` (plus the `#include <stdint.h>` needed for
`uint64_t`/`UINT64_MAX` there)

The non-hierarchical limit reader, the glibc-derived `dirname` helper, and
the hierarchical minimum-limit walker move from the procfs file into the
cgroup utility file with their bodies, headers and doc comments intact. They
read the memory controller through the registered `cgroup_ops` driver and
`safe_uint64`, both of which this file already uses for mounting and
walk-up helpers, so no other infrastructure is needed.

*Why this location:* the walkers are pure cgroup-hierarchy logic; the procfs
file used them only as an implementation detail of memory reporting.
Co-locating them with `cgroup_walkup_to_root` puts each hierarchy-walking
concern in one module.
*Production role:* they remain the only code that interprets per-cgroup
controller files into numeric limits, including the cgroup-v2 `max`/empty
"no limit" convention.

### 2. New `cgroup_get_memswap_budget()` resolver

`src/cgroups/cgroup_utils.c`, declaration and contract comment in
`src/cgroups/cgroup_utils.h`

A new exported function resolves the effective memory and swap budget of a
cgroup: the hierarchical minimum RAM limit, the current RAM usage, the
hierarchical minimum memory+swap limit, and the current memory+swap usage.
It returns each value through its own out-parameter and reports, through a
fifth one, whether the memory+swap pair could be resolved at all.

*Why this location and form:* the function is the shared procedure both
report paths were each re-implementing, so it lives where the other shared
cgroup-resolution helpers live and is the module's natural extension point.
The values are resolved in the order the readers historically used, and the
level of the result is the resolution level the callers want (bytes, RAM
fatal, memory+swap best-effort because swapaccount may be disabled on the
running kernel). The declaration duplicates the contract comment so API
readers see the failure semantics without opening the implementation file.
*Production role:* single snapshot point for "how much memory and swap does
this cgroup effectively have", used by container-visible memory reporting.

### 3. Swap-info helper rewired to consume the resolved budget

`src/proc_fuse.c`, `get_swap_info()`

The helper previously fetched the memory+swap limit and usage itself, then
derived the swap total/usage and resolved the swappiness hint. After the
move it receives the already-resolved budget — each of the four values as
its own parameter — plus the resolver's swap-accounting flag, keeps only the
swappiness read local, and writes the three derived swap values out through
individual pointers.

*Why this location and form:* the derivation (v1 subtracts the RAM values
from the memory+swap values; v2 reports the swap limit/usage directly) is
reporting knowledge and stays in the reporting layer, while raw fetching now
belongs to the cgroup layer. The signature grew one parameter per budget
value rather than taking the budget as one object — the shape this refactor
naturally produces before anyone introduces a bundling concept.
*Production role:* turns the resolved budget into the swap numbers both
`/proc/meminfo` and `/proc/swaps` report, including the
"swappiness hint defaults to 1 if unreadable" behavior.

### 4. Shared swap-total clamp policy

`src/proc_fuse.c`, new `apply_swap_policy()`

Both readers carried two verbatim copies of the same three-step policy — on
cgroups v1 the total swap is the RAM+SWAP accounting (so the reported
total includes the memsw-vs-mem limit headroom), clamp to the host swap
device size, and suppress the swap line to the usage when swappiness is
0. The copies are replaced by one helper next to the swap-info helper; its
comments explain the kernel-behavior rationale that previously lived in both
callers.

*Why this location and form:* deduplicating this block was part of the same
cleanup pass; each reader supplies its own memory-limit figure in its own
unit (the swaps reader divides its byte-level limit, the meminfo reader has
already converted and clamped against the host's MemTotal), so the policy
receives a pre-scaled limit value plus the derived swap numbers and hands
back the clamped total.
*Production role:* the single definition of how large the reported swap
pool is allowed to be, applied on the cgroup v1 report path and the
mounted-with-swap report path alike.

### 5. `/proc/swaps` reader

`src/proc_fuse.c`, `proc_swaps_read()`

The reader's inline RAM-limit and RAM-usage fetch block (with its three
`__do_free` string locals) is replaced by one call to the shared resolver,
and the swap total it reports is now computed through the swap-info and
clamp-policy helpers. The rest of the function — the offset handling, the
host `/proc/meminfo` scan for the host swap sizes, and the decision to emit
the `none virtual` swap-device row — is unchanged.

*Why this location:* the swap-device listing is the smaller consumer of the
budget; it uses the resolved limit/usage only to decide the size and used
columns of the single virtual swap device.
*Production role:* the `/proc/swaps` entry point of container swap
reporting.

### 6. `/proc/meminfo` reader

`src/proc_fuse.c`, `proc_meminfo_read()`

The reader's fetch sequence (RAM usage, memory-stat parse, RAM limit, swap
info) is replaced by the resolver call, with the memory-stat parsing left
where it is, and the SwapTotal/SwapFree branches delegate to the shared
clamp policy. The scalar locals it hands to the helpers grow accordingly.
The kB conversion of the RAM limit/usage stays exactly where it was — after
the swap numbers are derived and before the host `/proc/meminfo` lines are
rewritten — so the MemFree/MemAvailable arithmetic and the MemTotal clamp
against the host total operate on the same values as before.

*Why this location:* this is the primary consumer: per-field rewriting of
the host's meminfo, where the budget values are mixed with memory-stat
fields.
*Production role:* the `/proc/meminfo` entry point of container memory
reporting.

### 7. Associated minor edits

`src/proc_fuse.c` drops the now-unused fetch-string locals and the moved
helpers; `src/cgroups/cgroup_utils.h` gains the `stdint.h` include its new
declaration needs. The `libgen.h` include in `proc_fuse.c` stays (removing
it is unrelated cleanup). No FUSE operation signature, installed public API
beyond the cgroup utility header, or test code is part of this change.

## Behavior kept identical by design

The refactor is behavior-preserving by construction rather than by
re-testing: the resolver consults the memory controller in the same order
the readers used; a failed RAM resolution keeps producing the empty
`/proc/swaps` output and the host-complete `/proc/meminfo` fallback the
readers produced; unresolvable memory+swap values keep reporting an empty
swap budget with the swappiness hint left at its default; the byte-to-kB
boundary and the MemTotal clamp in the meminfo reader are untouched; and
the v1/v2 swap arithmetic, host-device clamp and swappiness suppression
moved verbatim into the shared helper.
