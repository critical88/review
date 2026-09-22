# `cargo-afl` startup, driver, and config flows are carrying copies of their own helpers

Hi — this bit me during the toolchain-label change and I'd like us to clean it up
before it bites the next person.

While landing the recent update-notice and config work, several of the small
helpers in `cargo-afl` and in the `cargo-afl-common` library it depends on were
folded bodily into the large functions that orchestrate three flows:

- the binary's startup and subcommand dispatch,
- the cargo driver path that reruns builds under AFL++ instrumentation, and
- the `config` subcommand orchestration in the shared library.

Those orchestrating functions now contain pasted-in statements that re-derive
things the crates already have helpers for — the rustc toolchain label and the
afl.rs package label, the xdg data-directory layout that decides where runtime
artifacts live, the probe for whether shared-object plugins are installed, the
string of `-Z llvm-plugins` flags, and the AFL++ version probe behind the
update notice. The originals mostly still exist next to the copies (several with
comments saying they are "not called anymore"), so the same knowledge now lives
in more than one place, and the copies use different intermediate variables and
their own commentary, which hides that they are the same logic.

That is exactly how I got caught: I adjusted one derivation, the unit tests
passed, and the built binary still disagreed because a second pasted-in
derivation inside one of the orchestrators kept producing the old label. Reading
those functions afterwards is also rough going — several hundred lines of mixed
labeling, directory resolution, process spawning, and flag assembly in a single
body.

Could we restructure these three flows so the orchestrators delegate again
instead of duplicating? I'd like to see:

- the crate's helper layer owning each of these concerns in exactly one place
  (label derivation, data/plugin directory resolution, the update notice, the
  plugin flag assembly), with the orchestrating functions calling those helpers;
- the config build orchestration composed of named steps again, the way the
  source-update pieces already are, instead of one body that does labeling,
  make invocation, and artifact installation inline;
- any "not called anymore" markers and the prose that describes the old
  arrangement cleaned up to match the final shape.

Please keep behavior and compatibility untouched:

- identical CLI behavior: subcommand dispatch, flags, help and usage text, and
  all emitted messages (including the update notice and its conditions) must
  not change;
- the on-disk layout the tooling produces must stay the same, and so must the
  make/environment details of the AFL++ build;
- no new panics on these paths; errors keep surfacing the way they do today;
- `cargo build -p cargo-afl` and the recorded test suite (the binary and `clap`
  test targets) must pass with the same skips. Note the unit tests call several
  of the crate helpers directly, so whatever shape you pick, the helpers the
  tests rely on must keep working for them.
