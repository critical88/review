# Injection design record — uftrace: kernel tracing and symbol probing responsibilities drifted out of their owner modules

## Maintenance motivation

uftrace v0.20-era work kept pulling on two seams of the codebase at once:

* how the `record` command sizes and drives kernel function-graph tracing across CPUs (per-cpu buffer sizing heuristics keyed on `--kernel-depth`, thread-count heuristics, per-cpu trace pipes under `tracefs`), and
* how replay-side utilities consume previously recorded kernel data and how the `graph` command matches user function names to recorded binaries (depth-trigger synthesis by name, backward compatibility with old data directories that lack a `kernel_header` file).

Both seams sit between command-level orchestration (cmds/) and module-level ownership (the kernel ftrace component in `utils/kernel.c`/`utils/kernel.h`, the symbol machinery in `utils/symbol.c`). Under normal maintenance pressure, it is easiest to grow the code where the *caller* lives: a sizing tweak lands right next to the heuristic it supports, a data-file convenience lands right next to the directory scan that needs it, a lookup lands right next to the trigger builder that consumes it. Nobody plans to relocate responsibility; it happens through a sequence of small, locally reasonable edits.

This case models that accretion faithfully: kernel writer/reader state manipulation and symbol-table probing that the kernel and symbol modules used to perform internally now live where their callers iterate.

## Modeled evolution (the normal development story being represented)

1. **Kernel writer buffer sizing work.** While tuning per-cpu buffer sizing against the `--kernel-depth` heuristics inside the record command's writer setup, the per-cpu descriptor construction (the `nr_cpus`/`traces`/`fds` array block) ends up applied by a small record-side helper right after the kernel module's tracing setup call rather than inside the kernel module's own setup function, where it used to live at the end.
2. **Trace-pipe opening and teardown under a --sort/restart hardening series.** During work that made the record command degrade gracefully when individual per-cpu pipes cannot be opened, the per-cpu open loop settles next to the record command's start-tracing step, and the record-side finish path gains its own drain/close/int-buffer-release step that runs before the kernel module's finish call. The kernel module's own start/finish functions keep the parts that operate on system-wide tracer toggling and file saving.
3. **Replay-side data directory rework.** While making the data-file layer able to open kernel data lazily and to tolerate old directories, kernel reader construction and teardown relocate into the data-file layer beside `open_data_file`/`close_data_file`, taking the kernel-cpu directory scan callbacks with them; the kernel-header loader they use gets surfaced through the kernel module's public header so the relocated code can still call it.
4. **Graph depth-trigger probing by name.** The graph command's trigger synthesis needs to know whether a user-supplied function name exists in a recorded module's symbol table. A byte-for-byte by-name probe plus its bsearch comparator is copied into the command file, so the graph command iterates its maps and probes each module's symbol table locally, while the symbol module's own probe stays for its existing users.

## Overall design

Five source files change. Three clusters of locations carry the movement, one struct family per cluster:

* **record-time kernel writer cluster (cmds/record.c, with the matching halves kept in utils/kernel.c)** — three helpers in the record command hold per-cpu array construction, per-cpu trace-pipe/output-file opening, and the record-time drain/close/release of those arrays. The owner functions in `utils/kernel.c` retain the tracer configuration, system toggle, and file-saving halves.
* **replay-time kernel reader cluster (utils/data-file.c + utils/kernel.c + utils/kernel.h)** — kernel reader construction and teardown move to the data-file layer together with the kernel-cpu directory scan callbacks; one supporting change exposes the kernel-header loader in the public kernel header.
* **symbol probing cluster (cmds/graph.c)** — the command carries a local by-name symbol-table probe and bsearch comparator used by its function-finding walk.

Cluster-by-cluster, the affected relation is between the same two kinds of components: a command/reader-side file that grows functions whose work is essentially the internal state manipulation of an encapsulated struct (`struct uftrace_kernel_writer`, `struct uftrace_kernel_reader`, `struct uftrace_symtab`) living in a different module's header, where the module that defines the struct already maintains the rest of that struct's logic. The shapes differ on purpose: a fragment hoisted out of a still-present owner function, whole-reader functions that emigrate with a small supporting header change, and a verbatim command-side copy beside live code.

## Cluster A — record-time kernel writer state manipulation (cmds/record.c + utils/kernel.c)

**What changed.**
* The per-cpu array construction block (cpu count, `traces`/`fds` arrays, -1 initialization) is removed from the tail of the kernel module's tracing setup function and becomes a record-side helper, applied right after the kernel setup call succeeds, operating on the record command's writer bundle and reaching into `struct uftrace_kernel_writer` through it.
* The per-cpu trace-pipe/<data>-file open loop (including the per-cpu error handling that closes what was opened) moves out of the kernel module's start function into a second record-side helper invoked as part of the record command's start-tracing step; the kernel module's start function keeps the tracer toggle and the enabled-state bookkeeping, and its own error path is trimmed down to match what remains there (the full clean-up that used to share the per-cpu loop now relies on the part that stays plus the record command's error path).
* The kernel module's finish function loses its per-cpu drain/close/free block to a third record-side helper that the record command's finish path now runs before calling the kernel module's finish; the remaining owner function keeps the file-saving and tracer-reset halves, guarded by its own enabled-state check as the kernel module's internal guard now lives at function top with the per-cpu work gone.
* The record-side helpers take the record command's own writer-bundle type, since the call sites already hold that context in hand — the helpers act on its embedded kernel writer object.

**Why these sites and shapes.** Each of the three record-time roles (initialize, open, drain/close) is a compact, cohesive block whose statements are meaningful at exactly one call site of the record command's writer lifecycle, which is what makes saying "this belongs next to its caller" plausible during the modeled tuning work. The fragment shape (owner function keeps half) was selected for the setup/finish steps because that is how such code actually drifts: the half the caller iterates moves local, the half the module still owns stays put. The open-loop extraction was selected because the record command's start error reporting (kernel tracing disabled + warning) is what motivated it. The writer-bundle parameterization reflects the natural C practice of passing the caller's context struct rather than re-deriving the target pointer at each call.

**Production role served.** These functions are on the record path for kernel function-graph tracing: buffer sizing and descriptor construction for per-cpu reading, opening per-cpu trace pipes and the `kernel-cpuXXX.dat` output files under the data directory, and draining the remaining trace data before closing and releasing the per-cpu state at the end of a session.

## Cluster B — replay-time kernel reader setup/teardown (utils/data-file.c + utils/kernel.c + utils/kernel.h)

**What changed.** The kernel reader construction function (kparser init, kernel-cpu directory scan with its filter/sort callbacks, kernel-header loading, per-cpu rstack list setup, parser handler registration) and the reader teardown function (per-cpu parser/rstack release, buffer frees, parser exit) are removed from `utils/kernel.c` and defined in `utils/data-file.c` next to the data-file open/close functions that invoke them; the small directory scan callbacks travel with them as file-local helpers. As a supporting change, the kernel-header loader loses its file-local linkage in the kernel module and is declared through the kernel module's public header, because the relocated reader construction still calls it.

**Why this site and shape.** The data-file layer is where the open-a-data-directory logic lives, and the reader construction/teardown is exactly what a "make the kernel data path part of data-file opening" change grabs first. The whole-function relocation shape was selected because these two functions are self-contained public API functions with a single dominant interest in `struct uftrace_kernel_reader`; the one-line header exposure is the kind of supporting edit such a relocation realistically needs. Keeping both functions public (rather than file-local) reflects that the kernel module's own unit-test region still calls them.

**Production role served.** Whenever a data directory with the KERNEL feature is opened for `replay`, `report`, `dump`, `graph`, `tui` or `script`, the reader construction parses the recorded kernel data files into per-cpu rstack state; teardown releases it when the data file is closed.

## Cluster C — symbol-name probing in the graph command (cmds/graph.c)

**What changed.** The by-name symbol-table probe (bsearch over the sorted name index when available, linear walk otherwise) and the small name comparator that bsearch needs are added to the graph command as command-local code, and the graph command's find-a-function walk probes each module's symbol table through its local copy, passing its own collector context into it. The symbol module's own probe remains where it was and is still used by the rest of the tree.

**Why this site and shape.** The graph command's depth-trigger synthesis is the only command-side logic that iterates module symbol tables to answer "does this user-given name exist in this recorded binary". Growing that probe in place is the tactically cheapest thing a developer does during such feature work, and the duplication shape (owner's original stays alive for its other callers) is exactly what such growth leaves behind. The comparator copy is forced by the original's file-local linkage. The collector-context parameterization mirrors how the probing walk would naturally receive the name it looks for.

**Production role served.** `uftrace graph --depth-trigger`/trigger synthesis resolves a user-supplied function name against each recorded module's symbol table before deciding which graph nodes to synthesize.

## Relationship to the rest of the tree

The kernel module still exports the same lifecycle entry points; the record command just does more of the per-cpu writer work itself before calling them, and the data-file layer now performs the reader construction itself. Nothing observable changes for ordinary use, and the tree continues to build and run the same way; what changes is *where* the encapsulated-state manipulation lives — spread between the components that define those structs and the components that happen to call them.
