# Injection design record — terrabase storage file-handle capability surface

## Maintenance motivation

The storage engine of terrabase performs every durable read, write, seek, length
query, fsync, and truncate through a small file-abstraction layer
(`server/src/engine/storage/common/interface/`). Two storage generations (the
"sdss_r1" file-format plumbing shared by both, and the raw v1 reader/writer) plus
the v2 journal all sit on that layer. While preparing the journal-completion and
file-upgrade work, a maintainer wants "one interface for a file" so that a generic
function can accept any file-like handle without having to name several traits in
every signature. The natural (and tempting) move is to collapse the four
single-purpose capability traits — sequential read, sequential write,
durability/truncation, and position/length access — into one trait, because in the
local-disk case every one of those operations is ultimately a method on
`std::fs::File`, so bundling them looks free.

## Evolution being modeled

The change models a plausible refactoring commit: a developer consolidates the
capability traits under a single "file I/O" trait with a doc comment saying handles
that cannot honor a direction "are expected to reject it at runtime", rewrites the
handle impls to implement the merged trait, collapses the per-direction generic
impl blocks of the tracked reader/writer wrappers into single blocks bounded by the
merged trait, rounds the generic call sites down to the new name, and updates every
importer. The commit compiles, the disk-backed paths keep behaving exactly as
before, and the regression only shows up in
the structural shape: handles that used to be *incapable* of a direction at the type
level are now obliged to answer for it, and callers that used to promise exactly
what they needed now promise everything. This is the classic way an ISP regression
lands in a real codebase: not as a new design, but as an aggressive "simplification"
of an existing segregated design.

## Overall design

The injection is one coherent edit to the file-handle capability surface, propagated
wherever that surface is imported:

1. **Interface module** — the four capability traits are merged into a single
  `FileIo` trait (nine methods) with the read-block and write-loop helpers kept as
   default bodies inside it; the disk handle's four impl blocks collapse into one;
   the blanket `Read`/`Write` fronting impls are replaced by a fronting impl through
   the existing local-file helper trait (extended to the buffered writer); a new
   explicit impl is added for the in-memory buffer handle that the metadata-block
   tests use; the local/virtual test-context bridge grows a single merged impl; and
   both buffered handles, which used to be direction-specialized *by construction*
   (the reader never implemented a write trait at all), are now obligated to answer
   for every direction.
2. **SDSS generation-1 file plumbing** — the file-spec metadata writer switches from
   demanding only the write direction from its generic handle to demanding the
   merged trait, and the tracked reader/writer wrappers' four per-direction impl
   blocks collapse into one block bounded by the merged trait.
3. **Storage v1 raw wrapper** — same collapse as (2) for the v1 generation's
   reader/writer wrapper.
4. **Importer surface** — the file-format upgrade module, the v2 raw journal, and
   the journal recovery test import the merged name instead of the per-direction
   trait names; a doc reference in the virtual-file-system checker utility is
   retargeted to the merged trait.

Everything stays behavior-preserving on the disk paths: every method body that
already worked keeps working, only now behind one trait, and the runtime rejection
stubs only fire for direction combinations that were previously unrepresentable.

## Per-cluster rationale

### Interface module: merged capability trait declaration

**What changed.** `FileRead`, `FileWrite`, `FileWriteExt`, and `FileExt` are replaced
by `pub trait FileIo` containing all nine methods, with the original section
comments kept as interior markers and the two loops (`fread_exact_block`,
`fwrite_all`/`fwrite_all_count`) preserved as default bodies of the merged trait.

**Why this site and shape.** The declaration is the head of every obligation in the
system: merging the traits here is the one edit that makes every other edit
necessary. Keeping the helper bodies as *defaults* inside the merged trait (rather
than free functions or a separate extension trait) is what makes the merged
obligation feel cohesive rather than obviously wrong — a reviewer sees a tidy
nine-method trait with two convenience helpers, not four smashed-together traits.
The doc comment ("Handles that cannot honor one of these directions are expected to
reject it at runtime") supplies the in-fiction justification a real author would
write for exactly this design.

**Production role.** This module is the only place the engine defines what a file
handle *is*; every consumer below inherits whatever obligation is declared here.

### Interface module: disk handle and local-file fronting

**What changed.** The disk handle's four impl blocks collapse into a single
`impl FileIo for File`; the blanket `impl<W: Write> FileWrite for W` /
`impl<R: Read> FileRead for R` fronting impls are removed and replaced by
`impl<Lf: LocalFile> FileIo for Lf`, with the `LocalFile` helper extended to the
buffered writer, and a new explicit `impl FileIo for Vec<u8>` is added.

**Why this site and shape.** The disk handle is the primary implementor and keeps
honest implementations of every direction, so it must be welded into the merged
impl to hold the whole construction together. The blanket-impl *removal* is what
makes the merge consequential rather than cosmetic: in the clean design, any
`Read`/`Write` type (including the in-memory buffer used by the metadata-block
tests) was a first-class handle for the directions it could actually perform. After
the merge, buffer-backed handles no longer qualify through std, so the in-memory
buffer must be made an explicit implementor of the merged trait — and, because a
grow-only byte buffer genuinely cannot honor a read-with-cursor discipline, three of
its nine methods are outright runtime rejections (`fread_exact`, `f_cursor`,
`f_seek_start`), while the rest are real bodies (append on write, resize on
truncate, honest length, no-op durability — plausible for memory).

**Production role.** These impls decide which types may act as files at all; the
fronting impl is the gate that every non-`std::fs` handle has to pass.

### Interface module: buffered reader and buffered writer handles

**What changed.** `BufferedReader` (which previously implemented only the read and
position/length traits) and `BufferedWriter` (which previously implemented *no*
capability trait — it exposes only plain flush/drop helpers) now both implement the
full merged trait. The reader keeps its honest read/position/length bodies and
gains four rejection stubs (write, fsync, fsync-data, truncate); the writer keeps
honest write/durability bodies and gains four rejection stubs (read, length,
cursor, seek).

**Why this site and shape.** These two types are the natural poster children of
interface segregation: they are opened for exactly one direction. In the clean design the
type system documents that specialization — a tracked reader simply cannot be
handed to a write call. Forcing the merged trait on them produces the purest form
of the smell: plausible-looking impl blocks whose "write" is a runtime `Err`. The
error kinds (`ErrorKind::Unsupported`) with handle-specific messages are what a
maintainer told to "implement the trait, reject what you cannot honor" would
realistically write, rather than `todo!()`/`panic!`, which would be glaring even in
a drive-by review.

**Production role.** These wrappers front every tracked SDSS read and write in both
engine generations; they are the highest-traffic handles in the system.

### Interface module: local/virtual test-context bridge

**What changed.** The four per-direction impls of the test-context bridge enum
collapse into one `#[cfg(test)] impl<Lf: FileIo> FileIo for AnyFile<Lf>` that
forwards every method to the local or virtual backing handle.

**Why this site and shape.** This enum is the switch that lets the same file code
run against a real disk or the in-memory crash-injecting virtual file system during
tests. It is included because the clean design keeps it segregated per direction —
a genuine fourth implementor of the capability family, completing the propagation
through the interface module's whole implementor set rather than stopping at the
convenient ones.

**Production role.** Test-time correctness vehicle for all disk-related tests,
including the journal recovery path.

### SDSS generation-1 file plumbing: metadata writer and tracked wrappers

**What changed.** The file-spec required method `_write_metadata` (and its default
body in the blanket impl for simple specs) changes its handle parameter from the
write-direction trait to the merged trait; in the tracked reader/writer module, the
four impl blocks over the generic handle parameter — read-oriented, read+position,
position-only, and write-oriented — collapse into a single block bounded by
`FileIo`, and the module imports the merged name.

**Why this site and shape.** This is where the smell crosses from implementation to
*demand*. The metadata writer only ever encodes a header; demanding a complete file
capability from it is textbook over-wide requirement, and it converts instantly
because the parameter is `impl`-generic (the metadata-block tests pass the
in-memory buffer, which under the clean design satisfied just the write direction
and now must pass the whole merged gate). The tracked reader/writer wrapper is the
most convincingly over-demanding consumer in the repository: it has nine accessor
methods, and in the clean design each one is gated by exactly the direction it
needs (a cursor accessor needs only position/length; a buffered write needs only
write). A single `F: FileIo` bound flattens that whole story so any caller of any
accessor must produce a handle that can do everything, and it makes the
nine-impl-block pattern of the clean design retroactively look like fussiness.

**Production role.** This module is the shared on-disk format plumbing for both
engine generations — metadata encoding, tracked checksummed reads/writes, seeks,
length, durability barriers, and truncation all pass through here.

### Storage v1 raw wrapper

**What changed.** The v1 generation's own reader/writer wrapper gets the identical
treatment to the generation-1 tracked wrappers: its four per-direction generic impl
blocks collapse into one `F: FileIo` block, and the import line switches to the
merged name.

**Why this site and shape.** The repository carries *two* full implementations of
the tracked-wrapper pattern (the shared one and the legacy raw one), and the
v1 path still ships (open/upgrade paths reference it). Including it prevents the
task from being answerable by fixing one wrapper family, and it demonstrates the
same relation in a genuinely different execution path (legacy open and recovery)
rather than a duplicated copy of the same code.

**Production role.** File open and tracked reads/writes for v1-generation storage
files, still used by compatibility and recovery paths.

### Importer surface: upgrades, v2 journal, recovery test, virtual-FS utility

**What changed.** The three importing modules and one test rewrite their `use`
lists from per-direction trait names to the merged name; a rustdoc cross-reference
in the crash-injection utility points at the merged trait's helper.

**Why this site and shape.** These edits carry no new logic of their own — they are
the downstream consequence of the name change and they make the merged interface
the single point of contact for three more components that consume file handles:
file-format upgrades, the v2 raw journal, and the crash-injecting virtual file
system used by journal recovery tests. The retargeted doc reference keeps
intra-doc links resolvable, which a real consolidating commit would fix in passing.

**Production role.** Each is a distinct entry point into file-capability code:
format upgrades (length + write + durability), journal open/replay (read + position),
and the test harness used by all of them.

## What was deliberately left alone

The std-fronting semantics of real disk files, the virtual file system itself, the
checksum and encoding machinery, the query and index layers, and the network stack
are untouched: the edit stays inside the file-handle capability surface and its
importers, because stretching it into those systems would have produced an
unrelated grab-bag commit instead of one believable maintenance change.
