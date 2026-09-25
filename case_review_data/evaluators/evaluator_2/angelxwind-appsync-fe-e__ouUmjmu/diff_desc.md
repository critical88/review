# Change record: embedded-signature handling during Mach-O validation and entitlement copying

## Maintenance motivation

amfid (the patched validation daemon this toolset supports) is routinely handed
binaries that are corrupt, truncated, or signed with a hash flavor the daemon
cannot digest. When such an image arrives today, the failure is discovered late:
`compute_cdhash` maps the file, walks the load commands, finds the
`LC_CODE_SIGNATURE` command, and only then hands the raw signature data to the
blob parsing code, which may spend a full digest computation — or several, one
per candidate code directory — before concluding the image is unusable. For a
daemon that answers on a hot path, declining obviously unusable images earlier
is worthwhile, because the load-command walk in `macho_validate` already visits
the `LC_CODE_SIGNATURE` command and its `dataoff`/`datasize` window before any
hashing starts.

The second motivation comes from the entitlement dump tool. `dump.cpp` had
grown a mix of header-object code (the `MachHeader`/`FatHeader` classes that
own file walking and byte-swapping) and one long free function,
`copyEntitlementDataFromFile`, whose body still reached into raw on-disk
structures for the actual payload copying. Real maintenance on this file has
been progressively moving per-image work onto the image class so the free
function is left with file-level concerns only (opening, mapping, iterating over
fat slices, translating failures into `kCopyEnt*` codes). Extracting the
payload-copying step onto `MachHeader` is the same treatment.

## Development evolution being modeled

This change is what two ordinary hardening/refactor passes on this codebase look
like after they have been reviewed for local consistency but not re-examined as
a whole:

- In `cdhash.m`, early rejection was bolted onto the existing validation pass
  at the exact points where a signature is already reachable — the load-command
  walk in `macho_validate` for shape problems, and `compute_cdhash_macho` for
  hash-type viability — rather than being designed as its own pass through the
  file. Each writer read the blob structures directly at the point of use
  because that is what the surrounding style does: XNU's own
  `cs_validate_*`/`ubc_subr.c` code, which this file cites as its origin, walks
  these structures in exactly this register-by-register manner, and the file
  already contains the field-ordering conventions (`ntohl`, index-table access)
  needed to do it.
- In `dump.cpp`, a payload step that used to sit inline in
  `copyEntitlementDataFromFile` became an accessor on `MachHeader`, next to the
  other image-level operations (`GetLoadCommands`, `operator ->`).

Both halves are written the way incremental work on this repository actually
accrues: each site re-derives the offsets, sizes, and endianness handling it
needs from the structure definitions nearby rather than being threaded through
existing abstractions, because threading through an existing abstraction would
have meant changing the decomposition of `cdhash.m` first, which is a larger
step than the writer's immediate goal.

## Overall design

Two production files change, one per half of the work.

`AppSyncUnified-installd/cdhash.m` grows its two decision points. The
load-command walk gains an inspection of the first `LC_CODE_SIGNATURE` command:
after the existing bounds checks establish that the command and its
`dataoff`/`datasize` window lie inside the mapped file, the pointed-at bytes are
read as a superblob and the forgiving shape questions are asked — is this
really an embedded-signature superblob, does its length fit the declared window,
is the index count outside the 0x10000 bound XNU applies, and is it nonzero.
`compute_cdhash_macho`, between its in-bounds check on the signature window and
its delegation to `csblob_cdhash`, gains a scout that walks the superblob index
exactly the way the blob parser later will — including the code-directory
candidate test and the SHA1 < SHA256-truncated < SHA256 < SHA384 preference
table — but only far enough to answer one question: does any usable code
directory exist at all.

`AppSyncUnified-installd/dump.cpp` moves the entitlement extraction off the free
function onto the image class. `MachHeader` gains
`CopyEntitlementData(struct linkedit_data_command *signature, CFMutableDataRef
output)`, which resolves the signature's `dataoff` in the image's byte order and
copies each `CSSLOT_ENTITLEMENTS` payload into the caller's mutable data
object. `copyEntitlementDataFromFile` keeps everything that is file-level —
mapping, walking load commands, locating the signature command, and all
`kCopyEnt*` result handling — and delegates the extraction step, which shrinks
its body by the size of the moved walk.

## Per-cluster record

### `macho_validate`: signature-shape probe inside the load-command walk

This is where validation cost is actually spent — the walk already decrees how
large each command may be and whether it fits before anything else is
attempted. The probe sits inside the walk immediately after those bounds checks
so that a corrupt window is declined with the rest of the header rather than in a
second pass. Implementation choices and their reasons:

- The walk's two header words (`cmd`, `cmdsize`) are read once into locals at the
  top of each iteration and reused, the same reading discipline the other
  walkers in this file and in XNU's `ubc` code apply.
- Only the first `LC_CODE_SIGNATURE` command is examined (the `checked_cs_cmd`
  flag); the cdhash path below resolves at most one signature command, so
  probing duplicates would be wasted I/O on malformed inputs (multiple
  signature commands are permitted by the format but not consulted).
- The window check reuses the exact inequalities the file already applies
  elsewhere for pointer windows (`mh < data && data < end && end <= mh + size`),
  and rejecting only when the declared window falls outside the mapped image.
- The superblob questions re-ask what the blob parser will re-ask later: length
  versus the declared `datasize`, length not below a bare superblob header,
  count under the XNU bound, and count nonzero (an index-less superblob can
  never produce a digest). Anything that passes here is still fully parsed later
  by the blob layer; the probe only declines images that were already doomed.

The role it serves: early decline, before the image is used further in the
amfid pipeline, from the one place that already had the `dataoff`/`datasize`
window in hand.

### `compute_cdhash_macho`: hash-type viability scout ahead of the delegation

`compute_cdhash_macho` is the last Mach-O-file-specific step; after it,
`csblob_cdhash` takes over and starts consuming CPU on digests. A scout here can
stop the expensive path before it begins, and this is also the only spot that
knows how large the `dataoff`/`datasize` window is, so the scout lives between
the window check and the delegation. Implementation choices and their reasons:

- The walk mirrors the delegation's own code path: candidate selection uses the
  same code-directory slot test (`CSSLOT_CODEDIRECTORY` and the alternate range),
  the same `cs_codedirectory_validate` call for each candidate's length, and the
  same preference ranking XNU documents, so anything the scout calls usable is
  what the delegation would find usable, and anything it declines is a file the
  delegation would have failed after the digest work.
- Structure reads go through the same pointer arithmetic the delegation uses
  (`(uint8_t *)sb + offset`), so the scout's notion of where a candidate starts
  is the same one.
- Ranking by hand (`cd->hashType` compared against the four known flavors) rather
  than calling the file's rank helper: the point here is only whether some
  candidate survives, so the scout carries its local copy of the preference.
- Bounds questions inline and negative: superblob count above 0x10000, an index
  table that runs past the declared length, or an entry whose offset exceeds the
  declared length each decline the image rather than being recovered from.

The role it serves: decline, before the digest work, the images whose best
candidate hash flavor is unusable.

### `MachHeader::CopyEntitlementData`: the payload step becomes an image operation

The entitlement task is per-image (one signature per `MachHeader`), while the
free function's remaining concerns are per-file. The accessor keeps the
byte-ordering split that the file's two Swap worlds impose: the `dataoff` value
is resolved with the member `Swap` driven by the image's detected byte swap
state, because it lives in the image's load-command half of the file, while the
superblob and blob fields are resolved with the file-static `::Swap`, because
the blob lives on disk in network order like in `cdhash.m`, and the moved code
carries that distinction with it. The caller keeps the `signature != NULL`
shape, result-code assignment, and cleanup it already grew; only the extraction
loop is gone from it.

The role it serves: the image exposes "give me the entitlement payload
attached to me" the same way it already exposes its load commands.

## Deliberate variation across sites

The three worksites are independent edits, each shaped by the needs of the half
it belongs to rather than copies of one another: a validation probe nested
inside an existing walker (strict-subset questions only, guarded by first-match
semantics), a full reconnaissance-and-ranking scout ahead of a delegation
boundary, and a payload-extraction accessor moved onto a class that did not
previously touch that payload. Each one brought in the structure
knowledge it happened to need at the point of use — bounds questions in the
first, index-table walking plus hash-type preference in the second, slot
selection plus payload sizing in the third — and each keeps the local reading
conventions of the file it was written in (pointer windows and register-wise
structures in C, the image-class byte-swap split in C++).
