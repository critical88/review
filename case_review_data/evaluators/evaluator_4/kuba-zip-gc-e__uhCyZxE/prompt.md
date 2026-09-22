# Refactor request: stop the archive-session object from hoarding subsystem state

**Area**: the zip archive layer in `src/zip.c` — the session object that
`zip_open*`/`zip_stream_open*` hand out, the per-session active-entry state it
carries, and the module functions that operate on them. Build with the normal
CMake configure+build and run the `ctest` suites under `build/` to exercise the
behavior described below.

## What we keep running into

The session object has become the storage bin for every subsystem that needed
somewhere to put working state. Over several feature merges it picked up direct
ownership of three unrelated subsystems' mutable state, and we now pay for it
in every review and debugging session:

- **Password-entry encryption.** The PKWARE key schedule for the currently
  open entry — and the extra key sets the entry-close rewrite works with —
  are raw words laid flat on the session's record graph. The small keystore
  record that used to own this state, and the compress-callback state that
  went with it, are gone; the cipher helpers dig the fields out of the
  session handle instead.
- **The delete/compaction pass.** The whole archive walk that backs
  delete-by-name and delete-by-index — the entry-mark array, its element
  count, and the offset, length and physical-order scratch arrays plus the
  deleted flags — lives directly on the session, reserved around a walk.
- **Entry-name handling.** One session-owned scratch buffer with a capacity
  counter now backs every opened entry's name, verbatim or
  backslash-normalized, instead of the per-call copies that used to be made.

The consequences are structural, not hypothetical. Adding anything to one
subsystem means widening the session's record definition again; reasoning about
one subsystem means reading raw fields of a record that also carries two other
subsystems' state; session close has become the finalizer for all of it, so
getting the frees right requires knowing all three areas.

## Diagnosis

Ownership of subsystem working state has drifted onto the coordination object.
The session object should carry session identity, configuration and the
active entry — not another subsystem's scratch space. This erosion was applied
with the same recipe in several places, so when you find one spot where a
subsystem's working state sits flat on the session graph, look for the sibling
spots in the other lifecycle paths: plain and password opens over file,
in-memory-stream and `FILE*`-stream archives; entry scoping on open; entry
write and read for protected entries; the close-time rewrite of an
already-written protected entry; and delete/compaction over every archive
flavor. Address the pattern everywhere it appears in this layer, not only in
one subsystem.

## Desired outcome

Give each subsystem's working state back its own boundary. Concretely: the
mutable working state of these subsystems should live in small cohesive record
types that belong to the subsystem (or stay in per-call scope where it is
genuinely per-call), the functions that manage it should operate on that owner
rather than on session fields, and the session record should come out of this
carrying session-level concerns only. The extraction should hold for the
active-path key material, the fixup key sets used at close, the delete-walk
workspace, and the entry-name scratch — as well as anything else you find
following the same recipe in this layer.

## Compatibility boundary (must all hold)

- The public API in `src/zip.h` is unchanged: same functions, signatures,
  error codes and `zip_strerror()` strings; no new public symbols.
- Byte-identical outputs: archives written from the same inputs — plain and
  password-protected, stored and deflated, over files and over streams —
  must come out byte-for-byte equal to what the current code produces,
  including the bytes of protected entries and the rewrite the close path
  performs on them.
- All existing `ctest` suites keep passing in every supported compile-time
  feature configuration (`ZIP_ENABLE_INFLATE`/`ZIP_ENABLE_DEFLATE` on or off).
- Caller-visible memory ownership is unchanged: the library still frees
  exactly what it allocated on close, and caller-owned stream buffers are
  never reallocated by the library.
- The vendored DEFLATE/ZIP engine in `src/miniz.h` stays as it is; its
  feature guards keep the same compile-time story.
