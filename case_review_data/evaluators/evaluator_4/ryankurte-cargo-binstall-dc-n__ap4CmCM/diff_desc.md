## Maintenance motivation being modeled

The fetch pipeline in this repository builds binary fetchers (`QuickInstall`
and the GitHub-crate-meta fetcher) for a crate/version/target, lets the
resolver decide which fetcher can serve a package, and then lays out the
downloaded bin files. Constructing a fetcher needs one cohesive pack of
facts: the crate name and version, the repo URL, the target triple, the
package metadata, the template-rendering values, the shared repo-info
cache, and the signature policy.

At the pinned commit that pack is already carried behind an intermediate
construction struct (`Data` / `TargetData`), so each construction site
takes one value rather than enumerating the elements. The injection models
a plausible but heavy-handed maintenance refactor: a developer unwinds that
intermediate layer to "make each constructor explicit", passing the
fields straight through every boundary instead. The realistic motivation
is a perceived dislike of a wrapping allocation and bubbling indirection,
trading a thin owner for directness. Each step of the refactor looks
reasonable on its own; the aggregate effect is that the cohesive pack is
re-threaded element-by-element through many signatures.

## Evolution being modeled

The modeled evolution keeps behavior identical while changing only how the
construction facts travel: the wrapping struct is removed, its fields
become bare parameters on the trait method and the concrete constructors,
the orchestration and bin-collection helpers forward the fields
positionally, and the per-crate repo resolution moves from a method on the
wrapper to a standalone associated function on `RepoInfo`. No new feature,
no new test, no changed output -- only the threading of the pack.

## Overall design

The pack of fetcher-construction facts is re-exposed as bare parameters at
every site that previously received it through the wrapper, so the same
group of element types recurs across constructors and helpers in several
files instead of being carried by one value. The relation targeted by this
case is that recurring bare-parameter group.

## Per-cluster explanations

### `crates/binstalk-fetchers/src/lib.rs` -- trait construction surface

The `Data<'a>` struct and its `target_data()` accessor are removed. The
`Fetcher::new` trait method is given the construction facts directly as
positional parameters. The `RepoInfo::resolve` path that used to live as
`Data::get_repo_info` becomes a standalone `pub async fn resolve` on
`RepoInfo`, preserving the per-crate cache passing so resolution still
happens at most once per crate. This site defines the shared construction
surface every fetcher implements, which is why it is the first place the
group recurs.

### `crates/binstalk-fetchers/src/gh_crate_meta.rs` -- GitHub-crate-meta fetcher

`GhCrateMeta` stores the construction facts as bare fields and its
constructor takes them positionally, mirroring the trait surface. `find()`
resolves the repo through the new `RepoInfo::resolve`, and the context
builder is renamed to a positional `Context::from_parts`. Field accesses
that used to route through the wrapper now read the bare struct members.
This site is selected because the concrete constructor must duplicate the
trait's element list, which is the second crossing of the same group.

### `crates/binstalk-fetchers/src/quickinstall.rs` -- QuickInstall fetcher

`QuickInstall` grows bare fields to match the shared constructor signature
and forward the template-value accessor by deref. A couple of fields are
kept only to satisfy the constructor (suppressed as dead code), which is an
honest artifact of the unwinding: the fetcher does not itself consume every
element, but the shared surface still requires them. This site is the third
carrier of the same group and shows the cost of forcing a single concrete
fetcher to spell out facts it does not use.

### `crates/binstalk-bins/src/lib.rs` -- bin-file layout

`BinFile::new` takes the layout facts (name/version/repo/target/meta plus
template values and bin-path/install-path/template/no-symlinks) as bare
positional parameters, and `infer_bin_dir_template` keeps its existing
closure-based shape. This site is selected because the bin-file constructor
re-threads the same construction facts before computing source/dest layout,
a fourth crossing of the group at an otherwise unrelated boundary.

### `crates/binstalk/src/ops.rs` -- resolver type alias

The `Resolver` function-pointer alias is expanded so the resolver factory
matches the new bare construction surface. This is the seam that lets the
runtime `Resolver` construct fetchers against the unwound signatures.

### `crates/binstalk/src/ops/resolve.rs` -- resolution orchestration

A `push_fetcher_handle` helper forwards the whole construction pack (plus a
resolver predicate and the handle list) as a long bare parameter list, and
`collect_bin_files` threads the pack before delegating to `BinFile::new`.
The desired-target list becomes a tuple of (target, meta, template-values)
and the cargo-install fallback unpacks a triple. This site is selected
because the orchestration helper now forwards the pack element-by-element
to each fetcher constructor, which is the central place the elements must
stay in lockstep across the pipeline.
