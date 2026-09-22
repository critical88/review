# Session-anchored working state in the zip layer (`src/zip.c`)

## Maintenance motivation

Several feature branches for this library touch the same code paths and keep
needing one thing from the archive session: a buffer that outlives a single
call, state that must survive from entry-open to entry-close, or a helper that
would otherwise allocate fresh scratch on every archive walk. Reviewing those
branches, a maintainer looking for the smallest integration surface keeps
gravitating to one move: instead of threading per-call state through helper
signatures, put the working state on the session handle itself and make it
reachable from the `struct zip_t *` every function already receives. The
result is a flatter call protocol — every helper already takes `zip`, so new
state costs no new parameters — and fewer short-lived allocations because the
session can reuse one scratch area across entries and one preallocated
workspace across archive walks.

That convenience is the whole motivation, and it is entirely realistic in a
single-unit C library: the session handle becomes the one object everyone
stores their state on, and the small record types that used to own subsystem
state (or the stack locals that used to hold it for the duration of one
helper call) stop earning their keep. Nothing in the visible API changes; the
public header still exposes the same opaque handle and functions.

## The normal evolution being modeled

The work reads as the accumulation of three ordinary maintenance commits on
top of the current tree, each defensible on its own:

1. *Per-call cipher state folds into the entry record.* The PKWARE cipher was
   previously driven through a small keystore record type plus a companion
   callback-state record for the compressed-write path, with the close-time
   re-encryption fixup seeding its key sets in short-lived locals. With every
   cipher helper now reachable from the session, those record types are
   dissolved: the entry record takes the raw key words inline, the fixup key
   sets become persistent fields, and the deflate-callback wiring reads the
   keys straight off the entry. The helper functions gain `enc_` sibling
   variants that take the session handle, mirroring how the rest of the file
   already names its static helpers.

2. *The delete/compaction walk becomes session state.* The entry-deletion
   pass previously built its walk state per invocation. A maintainer fixing
   allocation churn in repeated-delete workloads hoists the whole walk
   workspace — the entry-mark array, the offset and length scratch arrays,
   the physical-order scratch, the deleted flags, and the element count —
   onto the session, reserved at the start of a delete pass and released when
   the pass completes, and the marking/ranking/finalize helpers become
   session methods.

3. *Entry names settle into a session scratch.* Opening an entry previously
   allocated a fresh name copy per call site (verbatim for lookup paths,
   backslash-normalized for creation paths). Folding those copies into one
   session-owned, capacity-tracked scratch buffer means opening an entry
   never pays a fresh allocation after the first grow, and the terse
   string-helper utilities the copies used to lean on stop being needed.

In each step the session record accumulates the state; in no step does the
public API or the produced archive bytes change.

## Overall design

- `struct zip_entry_t` (the per-session active-entry state already embedded
  in `struct zip_t`) gains the live PKWARE cipher key triplet, the two
  re-encryption key triplets the entry-close rewrite needs, and the offset of
  the already-written keystore header.
- `struct zip_t` gains the session-owned delete/compaction workspace (entry
  mark array, element count, offset scratch, length scratch, physical-order
  scratch, deleted flags) and the session-owned entry-name scratch (buffer
  plus capacity).
- Every subsystem helper is rewritten as a function taking `struct zip_t *zip`
  and operating on those fields through the handle: the `zip_entry_enc_*`
  cipher family, the `zip_delete_workspace_*` and marking/finalize family,
  and the `zip_entry_name_*` family.
- Lifecycle integrators (`_zip_entry_open`, `zip_entry_open*`, `zip_entry_close`,
  `zip_entry_write`, `zip_entry_decrypt_and_read`, `zip_entries_delete*`,
  `zip_close`, `zip_stream_close`) wire the three absorbed state areas in:
  cipher init/seeding at entry open, re-encryption at close, workspace
  reserve/release and compaction across delete paths, scratch growth at name
  store, and scratch/workspace teardown at session close.
- The dedicated keystore record, the deflate-callback state record, and the
  per-call string utilities (`zip_strclone`, `zip_strrpl`) disappear; their
  behavior is absorbed by the session methods above. The entry-mark element
  type stays, since the mark array still needs its element layout.
- The vendored DEFLATE/ZIP engine (`src/miniz.h`) is untouched: feature
  guards (`ZIP_ENABLE_INFLATE`, `ZIP_ENABLE_DEFLATE`) around the absorbed
  state follow the same compile-time story as before.

## Per-cluster rationale

### Live cipher key triplet on the entry record

What changed: the keystore record that used to live as an embedded record on
`struct zip_entry_t` is replaced with three raw `mz_uint32` fields
(`enc_key0/1/2`), guarded `ZIP_ENABLE_INFLATE || ZIP_ENABLE_DEFLATE`, and the
key lifecycle helpers (`reset`, `update`, `keystream`, `password`,
`decrypt`, `encrypt`) are rewritten as `zip_entry_enc_*` functions seeding and
advancing those fields through the handle.

Why here and this form: the keys are per-entry state — seeded from the
session password at open, advanced per plaintext byte — so the entry record
is the natural home once the keystore type is gone. Raw words keep the
debugger story the flattening wanted: with a `zip` handle at a breakpoint
every key word is visible on the object being inspected.

Production role: every password-protected entry write advances these fields
once per data byte, and every password-protected read advances them while
decrypting, in both file-backed and memory-stream sessions.

### Close-time re-encryption key sets

What changed: the entry-close fixup that rewrites an already-written entry
(the local keystore header must be rewritten once the true CRC and sizes are
known) previously seeded two short-lived key sets — the original keys the
stored bytes were written with and the corrected keys the rewrite uses — in
stack locals of `zip_entry_close`. Those key sets become two persistent
triplets on the entry record (`enc_fix_*`, `enc_orig_*`), alongside
`enc_header_ofs`, the offset of the keystore header the fixup rewrites.

Why here and this form: promoting the fixup keys to fields keeps the entire
fixup observable from the session after close returns, and lets each seed /
stream step be its own small handle-taking function
(`zip_entry_enc_fix_password`, `zip_entry_enc_fix_encrypt`,
`zip_entry_enc_orig_password`, `zip_entry_enc_orig_decrypt`) instead of one
long wrapper holding locals. The triplets mirror the live-key field shape so
the file reads uniformly.

Production role: the fixup runs on every password-protected entry close that
wrote a compressed (DEFLATE) entry, over both file and stream archives; it is
the path that keeps encrypted bytes byte-exact when the archive is finalized.

### Session-owned delete/compaction workspace

What changed: `struct zip_t` gains six fields — the entry-mark array and its
element count, the offset scratch and length scratch arrays that feed the
compaction copy pass, the physical-order scratch used when the central
directory is sorted, and the deleted-flags array — together with
`zip_delete_workspace_reserve`/`zip_delete_workspace_release`, and the
marking, ranking (`zip_index_next`, `zip_sort`, `zip_index_update`),
finalize (`zip_entry_finalize`) and apply (`zip_entry_set`,
`zip_entry_setbyindex`, `zip_entries_delete_mark`) helpers reading the
workspace straight off the session.

Why here and this form: reserving the workspace once per delete pass instead
of per helper argument lets every step of the walk share one allocation, and
the reserve/release pair gives the pass a clean entry/exit. The six fields
travel together through the whole pass, so hoisting them as a set is the shape
a maintainer naturally reaches for.

Production role: deletion by name and by index, over file, memory-stream and
`FILE*`-stream archives, covering sorted and unsorted central directories
and large-archive (ZIP64) layouts, all run on this workspace.

### Session-owned entry-name scratch

What changed: the per-call name copies become one session buffer with a
tracked capacity, served by `zip_entry_name_set` (verbatim copy) and
`zip_entry_name_set_normalized` (backslash-to-slash on the way in, per the
format's path rules) and released by `zip_entry_name_release`, which detaches
the open entry before freeing. The buffer grows in place; the open entry
points into it until replaced.

Why here and this form: a heap-profile maintainer sees repeated same-size
allocations from every opened entry name; one reusable buffer is the standard
C answer. The normalize-on-store form keeps the creation paths identical to
what they produced before. The `zip_strclone`/`zip_strrpl` utilities exist
only for the old per-call copies and are absorbed.

Production role: every `zip_entry_open`/`zip_entry_openbyindex` scopes the
active entry through this scratch, and `zip_entry_name()` keeps returning the
same string contents it always did.

### Lifecycle integrators

What changed: `_zip_entry_open` seeds the cipher and stores the entry name in
the session scratch; `zip_entry_close` runs the re-encryption fixup on the
promoted key sets and detaches pointers; `zip_entry_write` and
`zip_entry_decrypt_and_read` advance/burn the live keys through the
handle-taking encrypt/decrypt steps; `zip_entries_delete` and
`zip_entries_deletebyindex` reserve the workspace, drive the walk on it, and
release it; `zip_close` and `zip_stream_close` free the scratch and any
reserved workspace alongside the session itself.

Why here: these are the seams through which the three absorbed areas meet
user-visible behavior; each integration point is where the hoisted state is
wired in and where its lifetime is anchored to the session the caller already
owns.

Production role: session open/entry open/read/write/close/delete/close across
plain and password-protected, file-backed and streaming use, in every
feature-selection compile configuration (INFLATE/DEFLATE on or off).

## Fidelity constraints carried through the design

The produced archive bytes, the public API in `src/zip.h`, error-code
propagation and messages, and caller-visible memory ownership are designed to
stay exactly as they were; the re-encryption fixup keeps deriving its
keystore salt the same way, so encrypted outputs remain byte-comparable
build-over-build, and the feature guards keep every compile-time configuration
building.
