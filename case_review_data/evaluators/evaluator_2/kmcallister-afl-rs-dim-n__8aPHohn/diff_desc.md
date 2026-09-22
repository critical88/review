# Injection design record — cargo-afl / afl.rs at e0b8df5

## Modeled maintenance motivation

The pinned revision sits at the end of a short sequence of startup-and-update
commits: upstream landed "warn about AFL++ updates" and then "emphasize AFL++
updates for agents", i.e. the maintainer spent this stretch of the project
working inside `cargo-afl`'s startup path and in `cargo-afl-common`'s config
orchestration. The changes in this diff model how a developer in that situation
degrades a helper architecture without ever deciding to: while iterating on a
specific flow, the easiest way to see every step in one place is to copy a
helper's body into the caller, keep reading top-to-bottom, and move on. The
original helper stays behind "for the tests" or "as a reference", the pasted
copy gets new intermediate variables and comments phrased for its new host, and
in the heat of that week nobody goes back to reconcile the two. Each individual
copy defensible in isolation ("it made the flow readable"), the accumulated
effect is a family of orchestrators that re-derive the crate's own knowledge in
parallel.

The diff reproduces that evolution faithfully, including its tell-tale
surface features: the originals are left alive and annotated
`not called anymore`-style with `#[allow(dead_code)]` where unit tests would
otherwise complain, the copies are not verbatim transplants but re-expressed
variants (`?` vs `unwrap()`, plain bindings vs `let ... else`, flat sequential
statements vs a builder chain), and each copy carries boundary comments that
describe a plausible-sounding decomposition of its host function rather than
acknowledging its origin.

## Overall design

Four orchestrating sites across the two crates that the recorded build compiles
received inline copies of their own helpers:

1. the plugin-presence probe in the shared library (consulted by the config
   fast path on every invocation),
2. the `config` subcommand implementation in the shared library,
3. the binary's `main` function, and
4. the binary's cargo driver function that reruns builds under instrumentation.

The copied pieces are the crate's "knowledge" helpers — toolchain label
derivation, package label derivation, data-directory resolution, plugin presence
scanning, plugin flag assembly, the AFL++ version probe, and the update notice —
plus, in the config cluster, the make-based build step and the runtime object
installation step. Copying label derivation and directory resolution into three
different sites gives the same knowledge several simultaneous owners and makes
every site's body large; copying the probe/notice/flag helpers re-expresses them
in slightly different Rust idioms so the copies do not read as obvious
transplants. What was deliberately *not* copied: the LLVM config probing, the
make invocation, and the update/clone steps around them remain calls and named
functions — those pieces carry real environment and version furniture the
modeled author would not want to untangle mid-feature.

Depth requirement: the four sites together embed copies of five different live
helpers (two labeling helpers reachable through the crate's data-directory
chain, a version probe, an update notice, and a flag assembly routine), so the
inlined levels stack multi deep — a caller that absorbed a helper which itself
wraps `rustc_version` probing plus `xdg` directory creation plus string
assembly, repeated in three combinations.

## Cluster A — the plugin-presence probe (shared library)

**What changed.** The presence probe previously walked the crate's helper chain
(version metadata probe → toolchain label, package-version label, `xdg`
data-directory creation, then a directory scan for `so` files). Its rewritten
body derives the toolchain label inline, builds the package label inline
(asserting the compile-time version string is non-empty exactly as the helper
does), creates the data directory through the same xdg prefix, and performs the
`so`-extension scan inline with an early `return` — reformulated from the
helper chain's style into a direct read-through.

**Why this site and shape.** This probe is the cheapest thing the binary's
fast path calls, so it is exactly where a developer re-reading the update flow
would land: "which directory are we even probing?" Being small, its copy is
the most verbatim and least restructured of the four sites. The chain of helpers it replaced is
what made the rewrite tempting: the probe used to compose four calls across
three layers of indirection (`afl_llvm_dir` → `data_dir` → the two label helpers); answering
"which directory" inline reads much more direct to the modeled author. The
comment block left above the rewritten probe narrates exactly that: the
data-directory scheme is spelled out, described as what the "old indirections"
produced, so the reader is encouraged to trust the inline version.

**Production role.** Determines whether a toolchain is "already configured" for
the config fast path — a startup-visible decision on every `cargo afl` run
through the config entry, and one of the two ways label/directory knowledge
escapes its owner in the shared library.

## Cluster B — the `config` subcommand (shared library)

**What changed.** The subcommand's body gained the same two label derivations
inline at the top (a second copy inside the same crate, differing in details:
`PathBuf::from(&toolchain_label)` with by-reference joins, `with_context`
wrappers), the runtime object destination computed inline from them, the
make-based AFL++ build/install step inlined as a `Command` builder chain
(`current_dir` / `args` / four env mutations / `env_remove`), the runtime
object copy performed inline, and the previous build/install step functions
annotated `#[allow(dead_code)]` with comments describing them as the previous
layout kept "for reference".

**Why this site and shape.** `config` is the crate's largest genuine function
even before the change, and the modeled author spent the week adding the
update flow to it: the most natural (worst) habit is to grow related
label/build/install work where you already are instead of calling the steps the
file already defines. The two label copies deliberately diverged in expression
(`PathBuf::from(&toolchain_label)` rather than by-value, context-wrapped fs
calls) so the cluster reads like an idiom-cleanup pass rather than a transplant.
The make invocation was re-idiomatized into a builder chain precisely so the
working directory, environment, and success check read together — a realistic
"modernization" a developer performs during such a fold-in — and the LLVM
config probe deliberately stayed a call: it has its own version/environment
furniture the author did not want to untangle.

**Production role.** The one function that builds the AFL++ toolchain the rest
of the repository depends on: it now owns labeling, directory resolution, source
update, build, install, and plugin copying in a single body.

## Cluster C — binary startup (`main`)

**What changed.** The startup body now: spawns and parses the installed AFL++
version probe itself as a local closure (`Option`-returning, immediately
invoked, `?`-hosted) instead of the crate's probe function; performs the
command decoration inline (the closure the crate's decoration helper contains,
kept as a local, applied with an `unwrap_or_default`); runs the update notice
inline as an immediately-invoked closure, re-expressed from plain bindings into
`let ... else` statements with a pre-bound `as_deref` step; and, when the
runtime wasn't built, derives the toolchain label and package label again,
creates the data directory inline (with `unwrap()` on the result), recomputes
the object-file path, and bails with the same "run `cargo afl config --build`"
message followed by an explicit `process::exit(1)`. The probe, decoration,
notice, and flag helpers remain in the file, `#[allow(dead_code)]`-annotated
with "kept for the unit tests" comments.

**Why this site and shape.** This models the actual upstream work most
directly: the update-notice feature was added here, and a developer shaping a
startup sequence ("probe version → decorate command → dispatch → maybe warn")
naturally drags each step's body up into `main` until the whole sequence reads
as one linear narrative. The `?`-vs-`unwrap()` and `let ... else` rewrites are
what an in-progress feature branch looks like: the local copy is written
fresh, in the style the author is currently reaching for, which is precisely why
it will not look textually identical to the original once reviewed later. The
closure formulation also gives the copies a home for early returns and
`Option`-chain `?` operators without touching `main`'s signature.

**Production role.** Entry point of the binary — the flow every user and every
agent invocation hits, and the second home of label/directory derivation plus
all of the update-notice knowledge.

## Cluster D — the cargo driver function (binary)

**What changed.** The function that reruns `cargo build`/`check` under
instrumentation now derives the toolchain and package labels itself (reusing
its existing version-metadata binding), creates the plugin data directory inline
(with `unwrap()`), scans the directory inline through a local `Result`-returning
closure, assembles the `-Z llvm-plugins=` rustflag string inline in a local
accumulation loop over the pass list (`afl-llvm-dict2file`, conditionally
cmplog, optionally ijon), and plugs the object file into the final link
argument directly. The flag-assembly helper remains, dead-code-annotated, with a
comment deferring to the unit tests.

**Why this site and shape.** This function is the binary's stated hot path, and
the driver's plugin handling is where the flags, the directory, and the runtime
object all meet; the modeled author inlined the whole side of it to keep the
gating check ("are plugins installed?") adjacent to the flags that depend on
it. The accumulation loop is the crate's original helper style, copied nearly
flat with renamed locals — the least "modernized" of the copies, standing in
for the brute-force copy a tired author makes at the end of the day. The pass
list and the ordering constraints are exactly what makes this copy
operationally dangerous to leave beside the original.

**Production role.** The instrumentation-carrying path for every downstream
fuzzing build; owns compiler flag assembly correctness (plugin pass order and
separation) and the runtime link argument.

## Deliberate notes on the copy style

- Several copies switch error-propagation style (`?` inside closures vs
  `unwrap()` at top level, `let ... else`, context-wrapped fs calls) so that
  the copies and their originals do not stay text-paired — matching how real
  maintenance drift accumulates.
- The originals were left alive `#[allow(dead_code)]`, not deleted, in all four
  clusters; two of them retain genuine production callers elsewhere, and the
  unit tests reference some directly. This is the conservative author behavior
  that most easily hides the problem: a reader has to notice the duplication;
  no failure signal announces it.
- No comment or marking in the diff describes the copies as temporary or
  problematic; the injected narrative comments present the inline arrangement
  as settled and describe the previous helper layout as the legacy shape. The
  `smoelius:` upstream comment style is continued for those notes, matching
  the repository's voice.
- Tests were not touched; the recorded suite covers the CLI contract and
  exercises several of the helpers directly, which is why the author keeps
  those helpers compilable rather than deleting them.
