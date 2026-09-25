# Injection design record — `smellgen-wacossusca34-glava-feat__74XjteV`

## Maintenance motivation

A user of the visualizer opens a new issue: "when I change the window size or the
requested sampling mode at launch, the audio session my render loop draws from is
still configured with the values I typed in an older config, and the two audio
backends re-implement the same rolling-buffer choreography with slightly
different local variables." The maintainer decides to deliver this as a single
feature: **audio sessions that are bound to the active renderer's requests**, plus
a cleanup of the duplicated streaming append/roll logic that the FIFO and
PulseAudio backends each carry inline. The change is motivated as consistency
("one place owns the session lifecycle, so every consumer uses it") rather than
as any kind of code-quality campaign, which is the normal way such work appears
in a real history.

## Modeled normal evolution

The natural implementation a maintainer reaches for in this codebase:

- The render API (`rd_*`) is the surface the process entry point already drives
  for every per-frame service (`rd_update`, `rd_time`), so extending that same
  surface with audio-session services feels like the path of least resistance:
  the entry point gets to "keep using the familiar API".
- The backends select and attach themselves through a registry
  (`struct audio_impl`), and their `init` callback receives only the shared
  audio session. Since the feature says "the session follows the renderer's
  requests", the maintainer widens that callback contract to also hand the
  renderer object itself to each backend, so each backend can "read its own
  configuration" at attach time.
- The duplicated buffer-roll code in both backends is hoisted into the new
  session services so each backend's streaming loop becomes decode + one call.

This evolution is coherent and arguably shippable; it also happens to place the
session lifecycle operations in a module that does not own the session struct,
and the renderer-request interpretation in modules that do not own the renderer
struct. Whether those placements are the dominant problem in this diff, how
many distinct responsibilities ended up misplaced, and how much of the diff is
incidental is deliberately left open here; this record documents the design
decisions, not a verdict.

## Overall design

Five coherent clusters:

1. **Audio-session services in the render core** (`glava/render.h`,
   `glava/render.c`): four new public functions next to the renderer request
   state — clear, roll+append, roll+silence, and the mutex-guarded drain —
   operating directly on the shared audio session struct.
2. **Backend contract evolution** (`glava/fifo.h`): the audio backend `init`
   callback signature gains a renderer argument; a forward declaration of the
   renderer type keeps the header self-contained.
3. **Backend request interpretation** (`glava/fifo.c`, `glava/pulse_input.c`):
   each backend learns to align its session's geometry and source with the
   renderer's requests at attach time.
4. **Streaming-thread deduplication** (`glava/fifo.c`, `glava/pulse_input.c`):
   both backends replace their inline roll-and-append blocks with the shared
   session service.
5. **Composition-root rewiring** (`glava/glava.c`): the entry point's inline
   zero loop and inline lock/copy/reset drain become calls into the new session
   service functions; `init` now also passes the renderer to the backend.

## Per-cluster rationale

### 1. Audio-session services in the render core (`glava/render.h`, `glava/render.c`)

**What changed.** `render.h` gains a forward declaration of the shared audio
session struct and four prototypes appended to the public renderer API block;
`render.c` gains an include of the audio backend contract header and the four
implementations, placed immediately after the window-backend registration
macros and before the GLSL bind-source section. Each implementation opens the
session struct's members directly: the zero service writes both frame buffers
sized by the session's buffer size; the roll services derive the chunk and
offset geometry from the session's buffer/sample sizes and `memmove` the two
frame buffers; the drain service takes the session's mutex, conditionally
copies both frame buffers out, and resets the session's modified flag.

**Why this site and shape.** The codebase convention is that anything the entry
point consumes per-frame lives in the `rd_*` family in the render core, next to
the renderer requests that parameterize it. Implementing the services there
follows the strongest precedent in the tree (every other render-loop service
the entry point calls is defined in this file), so the maintainer never feels
they are introducing anything foreign. The functions are non-static and
declared in the public header because both the entry point (a different file)
and the backends (two other files) must call them; a static helper would not
have been reachable, and this is the smallest surface that satisfies all three
consumers the feature created.

**Production role.** The four services become the sole implementation of
session-buffer traversal for the process: streaming threads append through
roll+append, the FIFO stall path pads through roll+silence, and the render loop
drains through the mutex-guarded read. The bodies are lifted or adapted from
the previously duplicated inline code (the entry point's zero loop and drain
block, and the backends' `memmove` pairs), keeping arithmetic and lock scope
byte-for-byte equivalent so the rendered signal cannot change.

### 2. Backend contract evolution (`glava/fifo.h`)

**What changed.** The audio backend registry contract's `init` callback type
now takes the renderer alongside the session; a comment explains that renderer
sessions are handed to the input layer at attach time.

**Why this site and shape.** The registry header is the one place every backend
and the entry point already include; changing the callback type here is the
minimal edit that makes the renderer reachable from every backend without
adding a constructor parameter to a public API. The commit must touch every
backend's `init` implementation anyway (the compiler enforces the new
signature), so the maintainer widens the contract here rather than inventing a
separate "configure" callback — one consistent edit instead of two mechanisms.

**Production role.** This is the attach-time wiring of feature cluster 3: it is
the reason every backend gains request-interpretation code in the same change.
It also silently evolves a shared contract that the OBS-style embedders of this
library follow, which is the kind of interface consequence this change carries.

### 3. Backend request interpretation (`glava/fifo.c`, `glava/pulse_input.c`)

**What changed.** `glava/fifo.c` gains `fifo_bind_session(audio, r)`, called at
the top of its `init`: it sets the session's buffer and sample sizes from the
renderer's corresponding requests and, when the renderer carries an explicit
non-default source string and the session has none yet, installs that source.
The default `/tmp/mpd.fifo` fallback remains in `init`. `glava/pulse_input.c`
gains a module-static renderer handle (mirroring the file's existing
module-static mainloop handle), stored by `init`, plus
`pulse_apply_session_requests(audio)`, called before `init`'s early return so
re-attachment re-aligns too: it sets the session's rate and sample size from
the renderer's requests and collapses the stereo/mono choice from the
renderer's input-mirror flag.

**Why this site and shape.** Both backends open their recording streams from
parameters that used to arrive via the session struct only, so aligning to
renderer requests must happen at attach time, in each backend, before the
stream is created. The two backends deliberately take structurally different
shapes: the FIFO helper receives the renderer as a function parameter (its
`init` is a plain function), while the PulseAudio helper closes over a
module-static handle the way that file already stores its mainloop — each
follows the strongest local idiom rather than a uniform pattern, so the change
reads as two developers following their file's conventions instead of one
template pasted twice. The PulseAudio helper sits before the early return so a
cached source name does not skip re-alignment, preserving the original
fallback semantics exactly.

**Production role.** This is the feature a user actually sees: a backend started
with a renderer that requests 1024-frame buffers, 44.1 kHz, and a specific
device opens exactly that stream, while a default renderer keeps today's
behavior (auto source detection, the same rates and sizes as before).

### 4. Streaming-thread deduplication (both backends' `entry`)

**What changed.** Each backend's streaming loop now decodes one read chunk into
frame-sized local arrays (`lbuf`/`rbuf`) with the original per-channel sample
arithmetic — the FIFO backend keeps its int16 conversion and integer
truncation order, the PulseAudio backend keeps its native float merge — and
then calls the shared roll+append service inside the same mutex window as the
removed inline code. The FIFO stall path calls the roll+silence service between
the same lock/unlock pair. The now-unused local cursor/offset/pointer variables
(`bl`, `br`, `fsz`, `q`, `buffer_offset`) are removed, and several
stray-trailing-whitespace lines in the touched blocks are normalized by the
editor as a side effect of the edit.

**Why this site and shape.** This is the honest cleanup half of the same
feature: the two backends each open-coded an identical buffer-roll with only
the decode loop differing, and the new session service is the natural place to
put it. Rolling into the service after the decode pass is the smallest
refactoring that keeps the lock window exactly as wide as before (roll and
append inside one critical section, as in the clean tree) while making the two
threads share one implementation. The decode stays local to each backend
because the raw formats genuinely differ.

**Production role.** The streaming threads keep their exact thread-protocol
behavior — one lock per chunk, modified-flag set, terminate check — while their
bodies now consist of format decode plus a single service call,
which is what makes the diff a plausible maintenance commit rather than a purely
synthetic insertion.

### 5. Composition-root rewiring (`glava/glava.c`)

**What changed.** The entry point's inline frame-buffer zeroing loop is
replaced by a call to the zero service right after the session struct is
initialized; the inline drain block (mutex lock, modified test, conditional
copies, flag reset, unlock) is replaced by a single call to the drain service;
and backend attach now passes the renderer object in addition to the session.

**Why this site and shape.** The entry point is the one module that legitimately
touches both the renderer and the session: it constructs the session from the
renderer's requests, picks the backend, and drains the session every frame.
Once the session services exist, its inline versions are the most visible
duplication in the file, so replacing them with the shared calls is the
routine follow-through a maintainer performs once helpers exist, without
changing any of its data flow.

**Production role.** The render loop's per-frame drain and the startup
clearing are now routed through the same services the streaming threads use,
so all three consumers of the session share one implementation.

## Deliberate structural variation

The change deliberately avoids a single repetitive template: the placement of
the session services follows the strongest precedent in the tree (`rd_*` in the
render core); the two backends' new request interpretation follows two
different local idioms (function parameter in one, module-static handle in the
other); the streaming deduplication touches two threads with genuinely
different decode loops; and the entry point's edits are call-site
simplifications rather than new logic. The functions also vary in granularity
(core services on multiple session members, request interpreters on multiple
request members, call sites merely delegating), which reflects the natural
texture of a feature branch rather than a uniform mechanical insertion.

## Scope honesty

The diff also contains work that is not placement-decision-bearing: the
whitespace normalization inside the touched streaming loops, the removal of
newly unused local variables, and the comment blocks that document the new
contract. Those are by-products of editing those functions, which is the usual
way incidental churn rides along in a real maintenance commit. Whether the
interesting placements above constitute one problem, several problems, or
mostly acceptable trade-offs is not decided here; each cluster above records
what was chosen and why so the reader can audit those decisions against the
diff.
