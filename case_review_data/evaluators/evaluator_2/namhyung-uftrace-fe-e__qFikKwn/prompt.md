# Kernel tracing and symbol probing responsibilities drifted out of their owner modules

## Maintainer observation

Reviewing the kernel tracing paths during v0.20 stabilization, we noticed that a lot of low-level state manipulation for kernel ftrace data no longer lives in the kernel tracing module. The record command now carries its own helpers that construct per-cpu buffer/trace file descriptors, open per-cpu trace pipes and output files, and drain/close/release the per-cpu trace state for the kernel writer; the kernel module's own setup/start/finish functions for that writer now contain only the remaining halves (tracer configuration, system-wide toggling, saving the recorded data). On the replay side, the kernel data reader's construction and teardown — including the kernel-cpu directory scan and the parser/rstack bookkeeping — have been moved into the data-file layer, with a supporting change that exposed the kernel-header loader through the public kernel header just so the relocated code could call it. The graph command, meanwhile, has grown a private by-name probe of module symbol tables, alongside the symbol module's own name-based lookup that the rest of the tree still uses.

This is a textbook ownership drift: several functions in the command and data-file layers have become dominated by the internal details of structs they do not own — the kernel writer/reader state maintained by the kernel tracing module, and the symbol-table state maintained by the symbol module. The modules that define and maintain those structs are the natural place for this manipulation: every other function that manipulates those fields this way already lives there, and the scattered copies leave the kernel/symbol writers and readers far colder and harder to keep correct than the rest of the tree.

## Desired outcome

Please untangle this drift and give responsibility for each component's encapsulated state back to the component that owns it:

* The kernel tracing module should again be the single place where kernel writer/reader internals are constructed, opened, drained, closed and released. The record command's writer lifecycle and the data-file layer's open/close flow should call into the kernel module rather than reaching into its structs, and the owner-side functions should carry complete logic again, not withered halves wrapped around externally performed steps.
* Any temporary exposures that exist in the public kernel header only to support the displaced code should be withdrawn once the logic is back with its owner, unless they are genuinely part of the module's intended surface.
* The graph command should go through the symbol module's name-based lookup for its function-name probing; command-local copies of that probing logic should not linger.
* Dead or duplicate logic left behind by the drift should be cleaned up rather than kept side by side with the restored owner-side implementation.

An equivalent restructuring that keeps behavior, the build, and the in-tree callers working while making the owning module perform its own state manipulation is acceptable; you do not have to reproduce any particular historical layout, function granularity, or naming. Restructure the code the way you judge cleanest, provided each owning module ends up responsible for its own data.

## Behavior and compatibility to preserve

* The full unit suite must keep its current results. (As before, the single `kernel_tracefs` unit case fails in this sandbox for environmental reasons; it is expected to keep doing so.)
* The public lifecycle entry points the rest of the tree consumes (kernel tracing setup/start/stop/finish on the record side, kernel data setup/teardown on the replay side, and the symbol module's lookups used by the traced-process runtime and in-tree consumers) must remain usable by all existing callers.
* No recorded/replayed data format change, CLI behavior change, or new global mutable state. The encapsulated structs stay with their owning headers.
* The record command's external behavior when kernel tracing is unavailable (disablement with the existing warnings) stays as it is today.
