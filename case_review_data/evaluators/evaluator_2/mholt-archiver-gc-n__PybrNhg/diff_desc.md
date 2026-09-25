# Injection design record — restoring a one-value manager in archiver v4

## Maintenance motivation

Archiver v4 deliberately un-bundled the library: instead of the single
`Archiver` value that v3 had (one type that registered formats and performed
archival/extraction), v4 exposes orthogonal pieces and expects callers to
compose them — `RegisterFormat`, `Identify`, `FilesFromDisk`, `FileSystem`,
plus the per-format `Archival`/`Extractor` interfaces.

Two recurring user reports motivate a maintainer to revisit that decision:

1. **Embedding programs want registries that are not package-global.** Today
   every format self-registers during `init()` into one package-level map.
   Embedders that only need, say, `.tar.gz` support still link every format
   in, and two copies of the library in one binary fight over the registry.
   They ask for a value they can construct and register a subset (or their
   own custom formats) against.
2. **Simple jobs became verbose.** "Archive this folder to `.tar.gz`" and
   "extract this archive into that directory" each take a page of
   composition code in v4, so users keep asking for the one-call behavior v3
   had — while keeping v4's clean, composable API available in parallel.

The maintainer's answer is a configurable manager value that owns a private
registry and a set of operational defaults: construct it, register formats,
tweak the defaults, and call the one-shot operations. Package-level
convenience functions continue to work through a shared default instance.

## Development evolution being modeled

The change is written the way such a feature actually grows, in four locally
sensible steps — not as one big-bang edit:

1. Introduce `ArchiveManager` with an empty private registry, a constructor,
   and a default instance the package functions delegate to (init-time
   self-registration of the built-in formats is untouched, so it now
   populates the default instance's registry).
2. Move registration and identification behind the manager, because
   identification is meaningless without a registry: the matching loops
   iterate the manager's registry, not a package map. A per-manager
   identification needs per-manager `identifyOne`/`readAtMost`, so those come
   along.
3. Add archive creation from disk (`ArchiveFolder`). It needs a format
   chosen from the destination filename plus a file list, so it pulls the
   disk-walk (`FilesFromDisk`) and its mapping helpers into the manager too,
   and adds a layout default (`ImplicitTopLevelFolder`) that the walk alone
   never had.
4. Add extraction to disk (`ExtractToFolder`, `writeFileToDisk`,
   `destinationPath`) with the two policy knobs asked for in that thread —
   refuse-or-truncate existing files, and log-and-continue per-entry errors —
   and route the virtual-file-system convenience through the manager so that
   managers can scope archive mounts into a subfolder (`Prefix`).

Each step is a coherent response to a request. The aggregate result is that
the manager becomes the only place the library's high-level operations exist,
carrying a registry plus five operational defaults — the maintenance tension
this case is built to exercise.

## Overall design

- **`ArchiveManager`** (new): a value type with a private `formats` map and
  exported policy fields (`FollowSymlinks`, `ClearAttributes`,
  `ImplicitTopLevelFolder`, `OverwriteExisting`, `ContinueOnError`,
  `Prefix`), all zero-valued by default.
- **`DefaultArchiveManager`**: the shared instance created at package scope;
  the package-level functions are thin delegates to it, so external callers
  and the CLI keep compiling and behaving unchanged. Built-in formats
  register into it during `init()` exactly as before.
- **`NewArchiveManager`**: starts empty; embedders register what they want
  (`RegisterFormat`, and lookups through `Identify`/`ByExtension`/
  `SupportedFormats`, which see only the private registry).
- The identification pipeline, disk walk, and fs presentation keep their
  bodies **where they previously lived** (formats.go, archiver.go, fs.go),
  now as methods, so format-matching knowledge stays next to the format
  declarations and submitters of new formats see minimal movement in review.
- Creation/extraction conveniences are written fresh in a new `manager.go`,
  because v4 had no single-call implementations to preserve.
- The old package-level `formats` variable survives as an alias of the
  default manager's map rather than being deleted, since existing
  enumerations over it must keep working.
- Zero-value policies are required to reproduce the pre-change behavior of
  every delegated package function (e.g. an empty `Prefix` must not alter
  mounted file-system paths).

## Location clusters and rationale

### 1. `formats.go` — registration and detection absorbed (4 methods)

`RegisterFormat` (duplicate-name panic preserved), `Identify` (both matching
loops), `identifyOne` (rewind, compressed-layer unwrapping, EOF tolerance),
and `readAtMost` (header sniffing) become methods of the manager; the
package-level names stay as one-line delegates because a dozen format
implementation files call `readAtMost` directly. What was `var formats` is
now the receiver's `formats` field; the package keeps `var formats =
DefaultArchiveManager.formats` (a map alias, so registration through the
default manager is immediately visible to older enumerations).

Why this shape: identification is registry-dependent, so per-manager
identification forces the matching pipeline to be reachable per manager.
Keeping the moved bodies in place minimizes review diff and keeps
per-format match subtleties (`.gz` header bytes, Brotli's lack of a magic
number, rewindable-stream invariants) near the formats they detect.
Production role: this is the library's front door for "what format is
this stream?" — the detection engine now consults whoever owns the
registry instead of package-global state.

### 2. `archiver.go` — on-disk gathering absorbed (3 methods)

`FilesFromDisk` (the `filepath.WalkDir` closure, symlink dereference and
preservation branches, attribute stripping via `noAttrFileInfo`),
`nameOnDiskToNameInArchive` (disk-to-archive path mapping), and
`streamSizeBySeeking` (seek-forward/seek-back sizing) become methods. The
walk gains one new behavior at the manager layer: when called with nil
options, the manager's `FollowSymlinks`/`ClearAttributes` fields supply the
defaults. Package-level delegates keep the frozen signatures (the repo's
own tests and downstream users call all three).

Why this shape: `ArchiveFolder` must gather with the manager's defaults, so
gathering becomes manager-reachable; the options parameter keeps its old
meaning for explicit callers, and nil-options callers get
backward-compatible behavior through the default manager's zero values.
Production role: turning directories into the neutral `FileInfo` list that
every archival format consumes — the bridge between the local disk and the
format-agnostic core.

### 3. `fs.go` — virtual-file-system presentation absorbed (1 method)

`FileSystem` becomes a method: directory shortcut, on-open identification via
the manager's `Identify`, size calculation via the manager's
`streamSizeBySeeking`, `ArchiveFS` construction for both path- and
stream-based mounts — now seeded with `Prefix: m.Prefix` on both branches so
a manager can scope mounts to a subfolder of the archive. The empty-by-
default prefix preserves the pre-change mount behavior.

Why this shape: the presentation layer already sits downstream of
identification, which now consults the manager's registry; attaching the
prefix here avoids threading an extra argument through the public `ArchiveFS`
API. Production role: the io/fs façade — directories, archives, compressed
files and streams all presented as one file-system abstraction.

### 4. `manager.go` (new) — the facade and its conveniences (6 methods)

- `SupportedFormats`: sorted registry listing (a small observability ask
  from embedders).
- `ByExtension`: resolves a filename against the private registry by peeling
  registered compression suffixes (`.tar.gz` → archive format ≀ compression)
  so the destination name alone chooses a full combined `Archive` format.
- `ArchiveFolder`: pick format from the destination name, gather inputs with
  the manager's defaults, honor `ImplicitTopLevelFolder` by re-rooting the
  gathered names under a same-named folder, then create/write/close the
  archive with careful close-order error handling.
- `ExtractToFolder`: identify the incoming stream through the manager's
  registry, extract via the format's `Extractor` interface with a
  `FileHandler` closure that writes entries to disk and honors
  `ContinueOnError` by logging and proceeding.
- `writeFileToDisk`: per-entry write-out (parent dirs, symlinks re-created as
  links, bodies copied) with `OverwriteExisting` selecting
  `O_TRUNC` versus `O_EXCL` collision handling, permission fallback for
  zero modes, and manual close error propagation.
- `destinationPath`: joins the entry name under the destination directory
  after `path.Clean` over an absolute join, so traversal (`..`, absolute,
  embedded separators) cannot escape the destination folder.

Why this shape: these are the one-call conveniences that motivated the work,
expressed over the manager's registry and policies so that embedders with
restricted registries get the same ergonomics. Production role: the
"one value does the common task" surface asked for by users; the path
confinement and collision flags record the operational review comments such
an extraction helper accumulates (zip-slip safety, "don't clobber my files
by default", "keep going past one bad entry").

## Deliberate structural variation

| Dimension | Variation chosen | Where visible |
| --- | --- | --- |
| absorption style | bodies kept at their original sites as methods vs. re-homed into the new file | formats.go/archiver.go/fs.go vs manager.go |
| API freeze mechanism | thin delegates, of deliberately different thinness (one-liner vs doc-bearing wrapper) | all four package files |
| registry continuity | map aliasing instead of identifier removal or accessor functions | formats.go |
| receiver kind | value receivers everywhere (a manager is copyable, all state is value/map/immutable) | all methods |
| policy carrying | manager fields + existing options struct, instead of replacing the options struct | archiver.go vs manager.go |
| format choice for creation | name-driven layered lookup (`ByExtension`) instead of stream sniffing (identification needs a stream, a destination filename has none) | manager.go |
| layout default | post-walk rewrite of the gathered name list vs. complicating the walk walker | ArchiveFolder |
| error policy | fail-fast default with opt-in log-and-continue using the standard logger | ExtractToFolder |
| collision policy | open-flags `O_EXCL` vs `O_TRUNC` at a single open site | writeFileToDisk |
| test-safety | confinement helper prefixing with "/" before `path.Clean` | destinationPath |

## Behavior and compatibility notes (design intent)

The intent throughout is that nothing observable by an existing caller
changes: signatures, documented semantics (duplicate registration panics,
`NoMatch` routing, nil-stream identification matching by name only,
rewindable-stream rules, WalkDir path mapping), init-time registration of
every built-in format, and the zero-value behavior of every exposed policy
are all preserved; the new conveniences are additive. The maintainer's
trade-off — one configurable value gathering several operational domains
under a single owner — is the intended subject of any downstream code
review of this diff; it is recorded here as design rationale, without
claiming anything about what the diff as a whole does or does not contain.
