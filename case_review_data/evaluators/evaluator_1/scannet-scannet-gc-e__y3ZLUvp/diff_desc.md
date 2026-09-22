# Injection design record — `SensReader/c++/src/sensorData.h`, `SensReader/c++/src/main.cpp`

This record documents the change applied to the pinned ScanNet checkout
(commit `3830fce7f8b2e48ef047ef7fd76ea5f62903f51c`) as an auditable design
rationale: the motivation behind it, the everyday code evolution it models, the
overall shape of the change, and the reason each production site was picked.

## Maintenance motivation

The SensReader library is the reference reader for the `.sens` capture format.
Over the lifetime of such a reader, periphery code accumulates around the type
that "represents a recording": helpers that were originally written beside their
data (frame buffers, calibration values, a sequence counter) migrate toward the
big central type as small "convenience" pushes — usually driven by a short-term
need to reach everything from one place (a GUI tool, a batch converter, a debug
demo) without threading a second include or a new dependency around. The change
modeled here is exactly that push: a series of local "just put it on SensorData"
decisions that leave the recording type holding the implementation of work that
does not belong to it, while the types that *do* own the data shrink into
pass-through shells.

No single one of these pushes looks alarming in review — each is small,
obviously correct, and saves the author a few lines. The change records them as
one evolution so the resulting structure can be studied as a whole.

## Evolution being modeled

The modeled history is a sequence of ordinary, individually defensible
refactors of the kind that appear in real commits:

1. **"Give the recording type a one-call API for its output folder."** The
   `saveToImages` write-out needs the output folder to exist. Instead of keeping
   the platform directory checks as free-standing namespace utilities, the
   recording type grows public helpers so callers never have to know about the
   `util` namespace.
2. **"Centralize the frame pipeline so every entry point shares one path."**
   The frame type's compress/free/decompress members move to the recording
   type as static workers, with every lifecycle entry point re-pointed to the
   shared implementation. The usual argument: one place to set a breakpoint,
   one place to add a format later.
3. **"Make the recording type the single source of derived values."** The
   intrinsic-matrix factory and the zero-padded frame-name formatter move next,
   leaving thin forwarders behind so existing call sites still compile.
4. **"Ship a decode demo with the library, callable without a free function."**
   The demo decode loop that lived in the reader program becomes a member so
   the same demo body can be reused by any consumer holding a recording object.

Each push keeps the code compiling and behaving identically; the interest of
the resulting tree is structural, not behavioral.

## Overall design

Two files are touched, both inside the SensReader library portion:

- `SensReader/c++/src/sensorData.h` — the pipeline header declaring `ml::SensorData`
  with its nested `RGBDFrame`, `CalibrationData`, `StringCounter`, and `IMUFrame`;
- `SensReader/c++/src/main.cpp` — the demo consumer program.

The change is behavior-preserving source motion plus re-qualification: bodies
move verbatim (adjusted only for member vs. free/static context and indentation
level), call sites are re-pointed, and no serialization format, numerical
value, allocation size, conditional branch, or public field changes. No build
system, no other tool under `SensReader/`, and no test code is touched.

## Cluster-by-cluster rationale

### Cluster A — platform directory checks become recording-type members

**What changed.** `ml::util::directoryExists` and `ml::util::makeDirectory`
(namespace-scope helpers, WIN32/LINUX conditionals intact) are removed from the
`util` namespace and re-homed as public statics of `ml::SensorData`. The
write-out member `SensorData::saveToImages` drops its `ml::util::`
qualification accordingly.

**Why this site and shape.** `saveToImages` is the only pipeline call site of the
two helpers in this header, so a "self-contained write-out" argument is easy to
make: an object that writes images into a folder may as well guarantee the
folder exists itself, without a second namespace import in the consumer's
mental model. Public statics (not instance members) mirror the original free
functions' statelessness and preserve the exact bodies, including the
`#ifdef WIN32` branches.

**Production role.** Filesystem preparation of the export target directory on
the image/pose write-out path.

### Cluster B — per-frame codec and buffer-ownership implementation moves to the recording type

**What changed.** `RGBDFrame`'s private implementation members —
`compressColor`, `compressDepth`, `freeColor`, `freeDepth`, and the six
format-specific decompression allocators
(`decompressColorAlloc_{stb,occ,raw}`, `decompressDepthAlloc_{stb,occ,raw}`) —
are deleted from the frame type and re-created in `ml::SensorData` as private
statics taking the frame explicitly
(`compressFrameColor(RGBDFrame& f, ...)`, `freeFrameColor(RGBDFrame& f)`,
`decompressFrameColorAlloc_stb(const RGBDFrame& f, ...)`, etc.). The frame's
public lifecycle — constructor, `replaceColor`, `replaceDepth`, `free()` — and
its two format dispatchers (`decompressColorAlloc`, `decompressDepthAlloc`) are
rewired to route through those statics (`SensorData::compressFrameColor(*this,
...)`, ...). The statics live in a new private section of `SensorData`,
gathered under a comment describing them as the shared per-frame pipeline.

**Why this site and shape.** The frame type is the natural victim of this
evolution: it owns the compressed buffers (`m_colorCompressed`,
`m_depthCompressed`, sizes, timestamps, camera pose) and all of the codec code
touches exactly those fields, so lifting the bodies only requires making the
implicit `this` an explicit `RGBDFrame&` parameter and prefixing field access —
a mechanical transformation a reviewer can verify by eye. Statics (not
instance members of `SensorData`) are the minimal-motion shape: they need no
recording object, so the single-include demo and the frame cache
read/write paths keep working unchanged. The dispatcher situation is kept
intact at the frame: dispatch on `COMPRESSION_TYPE_COLOR` /
`COMPRESSION_TYPE_DEPTH` still happens where it always did; only the target of
each branch changes.

**Production role.** Lifecycle mutation of per-frame state (construction,
replace, release) and the per-format decode/compress execution paths: raw /
JPEG / PNG color, raw / zlib / occi depth, with the guarded uplink branches and
the allocation sizes of each backend preserved byte-for-byte.

### Cluster C — intrinsic-matrix factory moves behind a forwarder

**What changed.** `CalibrationData::makeIntrinsicMatrix(fx, fy, mx, my)` keeps
its signature as a static member of the nested `CalibrationData`, but its body
is reduced to `return SensorData::makeIntrinsicMatrix(fx, fy, mx, my);`. The
matrix-construction implementation (the row/col assignments building the
calibration matrix from intrinsics) is re-homed as a private static of
`ml::SensorData` with the identical signature and body.

**Why this site and shape.** The nested-class layout makes this the cheapest
possible "centralize derived values" push: a nested class can call the
enclosing class's private statics directly, so the forwarder needs no extra
plumbing and every existing caller of the factory keeps compiling. The body is
parameter-pure (only depends on `fx, fy, mx, my`), so nothing else has to move
with it.

**Production role.** Derived calculation: the pinhole intrinsic matrix used
by calibration-holding consumers.

### Cluster D — zero-padded filename formatting moves behind a forwarder

**What changed.** `StringCounter::getCurrent()` keeps its signature as a member
of the nested `StringCounter`, but its body is reduced to a call to the new
`SensorData::buildFrameFilename(m_base, m_fileEnding, m_numCountDigits,
m_current)`. The padded-name implementation — the digit-count arithmetic and
the stream-based assembly that renders e.g. `frame-000123.color.ppm` — is
re-homed as a private static of `ml::SensorData`, with the member fields of the
counter now passed as parameters.

**Why this site and shape.** Same argument as cluster C, applied to the second
nested helper: the counter owns only four scalar fields and one formatting
rule, and once the intrinsics factory has moved behind the enclosing class,
moving the formatter too is a two-line diff in review. Passing the counter's
fields as parameters (rather than moving the counter itself) keeps the
write-out path's state where it was.

**Production role.** Zero-padded, sequence-numbered file naming on the
image/pose write-out path.

### Cluster E — the demo decode loop becomes a recording-type member

**What changed.** In `main.cpp`, the free demo function
`processFrame(const ml::SensorData& sd, size_t frameIdx)` (annotated as the
"how to decode .sens files" example) is deleted together with its call in
`main`. A member `SensorData::processFrame(size_t frameIdx) const`, body
verbatim, is added to the header, and the consumer now calls
`sd.processFrame(i)`.

**Why this site and shape.** This is the classic "the demo belongs to the
data" push: with the decode entry points already reachable from the type, the
shortest blog-post-quality example for users becomes
`sd.processFrame(0);` — no free function, no extra include in the consumer. The
member is `const` because the demo only reads frame data (it decompresses,
frees its own allocations, and touches no recording state), exactly matching
the free function's `const ml::SensorData&` parameter.

**Production role.** The documented consumer usage flow for decoding a frame
from a `.sens` recording: allocate-decompress-inspect-free.

## Scope decisions recorded for review

- **Chosen files.** `sensorData.h` and `main.cpp` are the only files in the
  repository that declare the reader pipeline types and a real consumer of
  them, respectively; they are the only places this evolution plausibly
  touches. The four sibling GUI/annotation tools under `SensReader/` each carry
  their own copy of the mLib helpers and do not include this header, so they
  share no compilation or execution path with the change.
- **Deliberately left alone.** `IMUFrame` and the frame cache classes are state
  containers without their own pipeline work; the `.sens` stream format members
  (`saveToFile` / `loadFromFile`) already belong to the recording type; and the
  `LiveSensorDataWriter` cache/threaded-writer machinery is a concurrency
  concern from a different subsystem that would require a separate story.
- **Behavioral stance.** Every moved body is moved verbatim. `#ifdef WIN32` /
  `LINUX` branches, allocation sizes, exception paths, field-for-field stream
  writes, and the caller-visible API of all four pipeline types are unchanged;
  the two consumer-visible call sites (`processFrame`, the directory check in
  `saveToImages`) keep their semantics with only the qualification of the call
  changing.

## Summary table

| Cluster | Moved concern | From | To | Rewired call sites |
| --- | --- | --- | --- | --- |
| A | platform directory existence/creation | `ml::util` namespace | `SensorData` public statics | `saveToImages` |
| B | frame codec + buffer ownership | `RGBDFrame` private members | `SensorData` private statics (`RGBDFrame&` parameter) | frame ctor, `replaceColor`, `replaceDepth`, `free()`, `decompressColorAlloc`, `decompressDepthAlloc` |
| C | intrinsic-matrix construction | `CalibrationData::makeIntrinsicMatrix` | `SensorData` private static | forwarder in `CalibrationData` |
| D | zero-padded name formatting | `StringCounter::getCurrent` | `SensorData` private static | forwarder in `StringCounter` |
| E | demo decode loop | free `processFrame` in `main.cpp` | `SensorData::processFrame` member | `main()` |
