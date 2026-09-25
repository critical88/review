# Design record — folding the zig toolchain pipeline into its two entry points

## Maintenance motivation

cargo-zigbuild carries a dense thicket of compatibility shims: zig version gates
that reach back before 0.12 and forward to 0.16+ (dlltool option filtering, the
`-undefined dynamic_lookup` mirroring, the `-lcharset`/`-liconv` pairing, the
`-lcompiler_rt` addition for windows-gnu), platform-specific files that must
exist on disk before cargo ever invokes a compiler (macOS text-based
`.tbd` stubs, the ARM `arm-features.h` write, the AEABI unaligned-access shim,
cmake toolchain modules, a no-op `otool` placeholder), and response-file
rewriting for the unusually long linker command lines rustc emits.

Trouble reports rarely respect that structure. A single issue such as "the
windows-gnu build picks the wrong dlltool after the 0.16 update" or "macOS
links against the wrong libiconv when the SDK path has spaces" actually cuts
across wrapper resolution, environment export, tool dispatch, argument
filtering, and on-disk support files. While bisecting a cluster of those
reports against several installed zig versions, the developer wanted the two
flows that matter — preparing the cargo environment, and invoking a zig tool —
to be readable end-to-end in one pass: every version gate, every platform
quirk, and every file that gets written visible exactly where it influences the
flow, with no jumps into helper after helper to reconstruct the overall shape.

## Normal evolution being modeled

This is the classic "temporary inlining that stays": helpers get folded into the
orchestration method during active debugging so the whole pipeline can be
traced in one place, with the intention of extracting things again once the
fires are out. The work described here is that folding pass applied to the zig
pipeline in `src/zig/`, left in the state it reaches when the investigation
winds down — the code still compiles, still passes the test suite, still
produces identical builds, but the delegation structure that made each concern
independently trackable is gone.

The folding was done the way a developer does it in practice: each absorbed
fragment was adapted to its new context (binding names matched the surrounding
method, guard style unified, per-region comments reworded to describe intent at
the new location, and abstractions that existed only to cross the old call
boundaries — small single-use predicates, a one-off cache-path helper — were
not re-created, since their whole meaning fits on one line next to their use).

## Overall design

The work concentrates on the two methods through which every cargo-zigbuild
build actually flows:

- `apply_command_env` (in `src/zig/cargo_env.rs`) — the environment-export step
  applied to every cargo invocation (build, check, test, run, doc, clippy,
  rustc, install).
- `execute_compiler` (in `src/zig/mod.rs`) — the dispatch for `zig cc`, `zig c++`
  and `zig dlltool` that assembles the final tool command line.

Around those two, three small modules lost helpers whose only remaining
consumers were inside the folded code (`src/zig/locate.rs`,
`src/zig/target_info.rs`, `src/zig/wrapper.rs`). No module outside
`src/zig/` is touched; helper-driven sections were deliberately kept outside
this folding work.

## Per-location rationale

### `src/zig/cargo_env.rs` — the environment-export host

`apply_command_env` already decided which per-target variables cargo would see
(`CC_<target>`, `CXX_<target>`, the linker override, `RANLIB_<target>`, `AR_<target>`
when zig ar is enabled, the `CARGO_ZIGBUILD_TARGET*` outputs). Everything it
delegated to now lives in its body:

- **Wrapper resolution.** The largest absorbed unit was the wrapper-preparation
  procedure from `wrapper.rs` (finding the wrapper for a target, deriving the
  zig triple, decoding the rustflags-encoded target CPU/features — including
  the cargo `--config` CLI override path — and producing the `zig cc`/`zig c++`
  wrapper paths). This sits at the top of the per-target loop because every
  later export (`CC`, `CXX`, linker, `AR`, the resolved `CARGO_ZIGBUILD_TARGET`
  output) consumes what it produces. Location and shape were chosen so the
  reader sees target parsing, triple derivation, flag decoding, and path
  materialization in the order they happen at build time.
- **OS-specific dependency files.** The macOS/ARM support-step (target-dir
  resolution, `.tbd` stub materialization for `libiconv`/`libcharset`, the
  `CACHEDIR.TAG` marker, the `arm-features.h` write for ARM gnueabi targets)
  moved in right where the per-target loop finishes with a target, since that
  is where the "does this target need extra files on disk" question is answered.
- **cmake toolchain integration.** The cmake toolchain file generation, its
  failure-tolerant export guard, the windows Ninja generator default and the
  no-op `otool` placeholder write followed directly, as they are all part of
  making third-party build systems cooperate with the exported environment.
- **Per-language compiler-option probing.** The built-in-option probing that
  shells out to `zig cc`/`zig c++` per source language, and the
  `BINDGEN_EXTRA_CLANG_ARGS` accumulation built from the probe's include paths
  and macro definitions, complete the export, since those arguments must
  reflect the zig in use, not the host clang.
- **Cache-path and cache-dir resolution.** The shared cache-directory helper
  from `locate.rs` was folded in (used for the linker wrapper dir and the
  SDK-dependent paths it spawns) rather than kept as a cross-module jump.

The exported observable behavior of this step is unchanged: the same variables
land in cargo's environment for every target/platform combination, along the
same paths, with the same accounting for pre-set values and for variables
inherited from an outer cargo zigbuild (which are outputs, must be scrubbed
before re-export, and are matched case-insensitively on Windows).

### `src/zig/wrapper.rs` — the resolution boundary dissolves, the writer stays

With the wrapper-preparation procedure folded into the environment-export host,
`prepare_zig_linker` and `prepare_zig_linker_with_cli_config` no longer exist as
separate units, and the module's imports shrink accordingly. The
rustflags-encoded target CPU/features decoding (`TargetFlags`) survives as a
data type — the folded code still needs it, so its visibility widens from
module-private to crate-visible; keeping the decode as a small, well-tested
unit while its orchestration was folded was judged fine at the time, since the
decode is self-contained data shaping with no platform or version policy in it.
The parts of this module that actually *write* wrapper scripts (the script
generation and per-shell quoting, triple-to-target mapping, and the symlink
fallback) were not touched: they are the tail of the pipeline, feed only from
the resolved wrapper request, and were outside this folding pass.

### `src/zig/mod.rs` — the compiler-dispatch host

`execute_compiler` assembled tool command lines from the arguments rustc
passes, delegating for every non-trivial part. Those parts are in its body now:

- **dlltool dispatch with version gating.** `zig dlltool` shares this dispatch;
  the `< 0.12` option filtering runs inline where the command arrives.
- **Response-file rewriting.** rustc moves long linker command lines into
  `@`-files (`linker-arguments`, with the msvc variant in utf16LE plus BOM);
  the decode-and-augment step — scan read, split on encoding, filter
  unsupported arguments, mirror `-undefined dynamic_lookup`, add the
  `-liconv`-implies-`-lcharset` pairing from zig 0.12 on, re-encode and rewrite
  — moved in next to the argument loop, since the question "what does the file
  on disk actually contain" is what decides several of the decorations.
- **Argument policy and platform decoration.** The macOS-specific argument
  preparation (SDKROOT export instead of `--sysroot` from zig 0.15, the
  `.tbd` deps directory management, the shim/cache-dir resolution for the
  AEABI case on ARM targets, the `-lcompiler_rt` addition for windows-gnu at
  zig 0.16+) sits after the argument loop, mirroring the order the compiler
  receives the arguments.

The two absorbed decision helpers behind the response-file handling —
"does this argument list pair `-liconv` with `-lcharset`" and "is
`-undefined dynamic_lookup` present" — were small enough that their logic
became part of the surrounding conditions rather than named units; the pairings
are now evaluated directly against both the response-file contents and the raw
command line. The version-gated command construction (finding the zig binary,
SDKROOT handling, stdlib arguments) at the end was likewise folded in.

### `src/zig/locate.rs` and `src/zig/target_info.rs` — leaf helpers removed

Two leaf units were retired rather than folded: the cache-directory resolution
(whose whole meaning is "honor `CARGO_ZIGBUILD_CACHE_DIR`, else the OS cache
dir, else cwd, then join the package name and version") and two one-line
target predicates (`is_mips32`, `is_windows_msvc`) whose single remaining
callers were inside the folded regions. Single-use one-line
abstractions of this kind are exactly what an inlining pass erases first: at
their call sites they are as clear as their names, and keeping them would have
meant preserving the jump-through scaffolding this pass was eliminating. The
zig lookup and version-validation logic that actually has multiple entry points
was left alone.

## What remains outside this folding

The module boundary list above shows the pass was restricted to the two
entry-point methods and the degenerate helpers feeding them. The Mach-O
install-name tooling, the zig binary lookup chain, the linker-argument
lookup tables, and the script generation internals kept their structure, as
did `execute_tool` for the `zig ar`/`ranlib` dispatch.
