# Injection design record — activity audit record scattered across the rawdrawandroid glue

Repository: `cnlohr/rawdrawandroid` at `d6da9633ffc0adf802ac27ef19f8999aeba7667b`.
Task ID: `cnlohr-rawdrawandroid-ss-n`. Smell category: shotgun surgery.
Language: C (`-std=gnu17`). Change surface: 4 production files, 235 insertions, 10 deletions.

## 1. Realistic maintenance motivation

rawdrawandroid is a minimal NativeActivity harness: `android_native_app_glue.[ch]`
run the app's event loop on a dedicated thread, feed lifecycle events into a
command pipe, and let the application (through CNFG/rawdraw) attach a window,
an input queue, a WebView and optional USB devices. Because applications built
on it ship as black boxes, the maintainer's recurring nightmare is the field
report of the form "the app showed a black screen and then Android said it
stopped" — a report that carries no trace of what the activity actually did.

The need this diff models is a *per-activity record of what just happened*:
when a user submits a problem report, support wants to reconstruct the
sequence of recorded steps an activity went through — cold launch or restore,
window gained or lost, focus changes, a permission round for a USB device, a
WebView that was created and then never finished loading. For an embedded
hobby framework without crash-reporting infrastructure, the natural
implementation is deliberately primitive: bookkeeping fields carried on the
`struct android_app` that every subsystem already holds a pointer to, updated
in place at each recorded step.

## 2. The normal development evolution being modeled

Real audit/tracing features almost never land as one reviewed design; they
accrete. A first version records "what the activity is doing right now" and
gets written wherever the first contributor happened to be standing: the
lifecycle callbacks. A field report about resume weirdness adds the reset on
resume. Someone notices the record floods when progress is polled or the
window is redrawn, and a folding counter appears — but only at the sites that
person knew about. Someone else wants to know *when* a step happened and
pastes a `clock_gettime` block at their two sites of interest. The USB
permission flow and the WebView shim are technically separate helpers with
separate maintainers, so when they gained recording it was written in their
own local idiom, through their own reachable handle (`gapp`), with their own
stage tags.

This diff produces the endpoint of that evolution in one state: 38 functions
across the glue, the USB helper and the WebView implementation block each
attach, classify, count, fold and stamp the record in their own words. Each
individual addition is defensible in the PR it arrived in. The result is that
the record's protocol — what a step is worth, when entries fold, what the
timestamps mean, which stages exist — is restated in dozens of slightly
different dialects, so there is no single place where the protocol lives.

## 3. Overall injected design

The record consists of:

* four fields appended to the end of `struct android_app`
  (`android_native_app_glue.h`), after the existing private glue fields so the
  layout of every existing consumer is untouched:
  * `auditLastStage` (int8_t) — stage tag of the most recently filed entry;
  * `auditTransitions` (int32_t) — entries counted since launch;
  * `auditStampMs` (int64_t) — CLOCK_MONOTONIC milliseconds of the last
    stamped entry;
  * `auditSuppressed` (int32_t) — entries folded into the previous one.
* a second stage vocabulary (`AUDIT_STAGE_*`, values 100–106) for entries that
  do not correspond one-to-one to an `APP_CMD_*` value — in particular the
  entries filed from outside the glue (USB round, WebView hop, UI-thread
  dispatch) and the derived launch/restore/teardown/anomalous tags.

The record is diagnostic-only by construction: `audit*` fields are written
everywhere and read only inside expressions that write them again (fold
decisions keying off `auditLastStage` or `auditStampMs`), so nothing in the
harness can steer observable behavior off the record. The writes are ordinary
integer stores; the existing control flow of every touched function — mutex
and cond usage, pipe protocol, callback delegation — is unchanged, and the
couple of callback bodies that were restructured were restructured only so a
local `android_app*` exists to write the record through, with the same
delegation calls in the same order.

An `#include <time.h>` was added to `android_native_app_glue.c` for the
timestamp sites.

## 4. Per-cluster rationale

### 4.1 The record definition (`android_native_app_glue.h`)

The glue header is the only struct every three entry-filing subsystems can
already reach — the glue by definition, the USB helper and the WebView shim
through the global `gapp`. Appending the four fields at the end of the struct
(rather than a separate global) is what makes the record per-activity, which is
the entire point for field support: two activities in one process must not
share a transcript. The `AUDIT_STAGE_*` block sits directly after the
`APP_CMD_*` enum it extends, because at least two of its tags (`LAUNCH`,
`RESTORED`, `TEARDOWN`, `ANOMALOUS`) are regularly mis-assumed to be command
values; giving them a distinct 100-base range documents that they are tags of
the record, not pipe commands.

### 4.2 App-thread command path: `android_app_read_cmd`, `android_app_pre_exec_cmd`, `android_app_post_exec_cmd`, `process_cmd`

This path is where the app thread observes lifecycle transitions, so it is the
densest cluster — the entry protocol appears in six of the seven pre-exec
cases and in three post-exec cases. Each case was written the way the
hypothetical contributor of that case would have written it, which is why the
shapes differ:

* `android_app_read_cmd` is the only site that records on *both* pipe
  outcomes, because an empty command pipe is itself a field-report smell; it
  folds consecutive identical polled steps with a boolean add
  (`auditSuppressed += ( auditLastStage == cmd );`).
* the `APP_CMD_INIT_WINDOW` case in pre-exec and the teardown in
  `android_app_destroy` each carry their own timestamp block — accreted
  history, not one shared helper, is the whole point of the modeled evolution;
  identical millisecond math is restated four times in the file.
* the `{PAUSE,STOP,...}` case gates the increment on the recorded stage
  actually moving (`if( auditLastStage != cmd ) auditTransitions++; else
  auditSuppressed++;`) — a read-modify-write coupling that exists in exactly
  two places.
* post-exec's `APP_CMD_SAVE_STATE` case spells the increment as
  `auditTransitions = auditTransitions + 1;`, a third textual form of "count
  one entry" that coexists in the same file with `++` and `+= 1`.
* post-exec's `APP_CMD_RESUME` case *resets* `auditSuppressed` after tagging —
  the resume-semantics change mentioned above, applied at one site.
* `process_cmd` stamps and counts at the top of the dispatch using the polled
  `cmd` as the tag, an eighth variant that classifies by raw command value at a
  different protocol layer than read_cmd's fold.

### 4.3 Lifecycle acknowledgement loops: `android_app_write_cmd`, `android_app_set_input`, `android_app_set_window`, `android_app_set_activity_state`

These four functions are the main thread's side of the pipe: request a change,
wait until the app thread confirms it. Recording here captures requests the
app thread may never poll. `android_app_set_window` tags with a conditional
expression (`( window != NULL ) ? APP_CMD_INIT_WINDOW : APP_CMD_TERM_WINDOW`)
because the raw parameter is enough to classify the direction; entry was filed
after the wait loop exits so the entry reflects the confirmed state, not the
request — a distinction the recorded prose in that comment preserves.

### 4.4 Launch, teardown and the two threads of the pipe: `android_app_entry`, `android_app_create`, `android_app_destroy`, `android_app_free`, `free_saved_state`, `process_ui`, `RunCallbackOnUIThread`

`android_app_entry` is frame zero — the only place the record is intentionally
initialized (`auditLastStage = LAUNCH; auditTransitions = 0; auditSuppressed =
0;` plus a stamp), because "entries since launch" is meaningless without it.
`android_app_create` distinguishes cold start from restore with an if/else on
the `savedState` parameter — the single fact field support asks for first —
while `android_app_destroy` and `android_app_free` both tag teardown at their
own points plus a stamp in destroy, so a teardown interrupted in between is
visible. `free_saved_state` records the SAVE_STATE handover because support
cannot otherwise tell whether a crash happened before or after the state
drop.

The UI-thread handoff (`RunCallbackOnUIThread` producing, `process_ui`
consuming, in this fork of the glue) appears on *both* sides with the same
`AUDIT_STAGE_UI_DISPATCH` tag and its own count, written in the tab-indented
style of their respective thread scopes. Duplicating the entry across the
producer and consumer of a pipe models the real evolution: whichever side
gained recording second did not refactor the first.

### 4.5 Main-thread lifecycle callbacks: `onDestroy`, `onStart`, `onResume`, `onSaveInstanceState`, `onPause`, `onStop`

The fifteen native-activity callbacks are invoked with an
`ANativeActivity*`, not an `android_app*`, so this cluster is where the
injected code exercises every reachable access path realistically: some
callbacks keep the original cast expression
(`((struct android_app*)activity->instance)->audit... = ...`, i.e. onResume,
onPause, the redraw fold), while the others promote a
`struct android_app* android_app = (struct android_app*)activity->instance;`
local first, exactly the kind of half-migrated style accretion produces.
`onSaveInstanceState` duplicates the fold-or-count decision of the pre-exec
lifecycle case (the app thread may already have filed SAVE_STATE), which —
together with read_cmd's fold — makes the fold policy restated three times
with three different key fields.

### 4.6 Tag-only observations: `onConfigurationChanged`, `onLowMemory`

Config reloads and low-memory notices have no meaningful "count" for the
transcript — one or a hundred look identical — so these two file the minimum
entries: a stage tag and nothing else, letting the app thread's counting cover
them. They participate in the same protocol at a different weight, which is
part of the spread the case models: there is no statement of what an entry
must contain that these sites would satisfy by shared construction.

### 4.7 Focus, window and input-queue observations: `onWindowFocusChanged`, `onNativeWindowCreated`, `onNativeWindowDestroyed`, `onInputQueueCreated`, `onInputQueueDestroyed`, `onNativeWindowRedrawNeeded`

`onWindowFocusChanged` types its stage tag with the `focused` parameter and
then counts conditionally off a *different* record field
(`auditTransitions += auditStampMs ? 1 : 0;`), the kind of cross-field coupling
that only a per-site rewrite can invent. `onNativeWindowCreated` carries the
file's fourth copy of the timestamp block (through a freshly promoted local);
`onNativeWindowDestroyed` does not, matching "window gain time matters,
teardown time is recoverable". The two input-queue callbacks record with the
identical tag (`APP_CMD_INPUT_CHANGED`) and increment on *both* the created and
destroyed directions — a real transcription ambiguity for support reading the
record, left in because distinguishing them is exactly the kind of protocol
decision that should be made where the protocol lives. `onNativeWindowRedrawNeeded`
only increments `auditSuppressed` — redraw storms would dwarf the transcript
and a redraw moves no stage — the opposite corner (fold-only) of
`onConfigurationChanged`'s tag-only corner.

### 4.8 USB permission flow: `DisconnectUSB`, `RequestPermissionOrGetConnectionFD` (`android_usb_devices.c`)

Both functions of the file are external to the glue but hold the global
`gapp`, so when the flow gained recording it went through that handle with the
glue-header `AUDIT_STAGE_USB_EVENT` tag — written in this file's tab style.
`DisconnectUSB` files the disconnect beside the reconnect backoff it explains;
`RequestPermissionOrGetConnectionFD` files at two internal points — before any
Java objects are touched, and a fold increment in the `!deviceConnection`
refusal branch — because from support's point of view a permission round that
was *refused* is the interesting entry, yet it is invisible in a tag.
Including the whole file also makes the cluster boundary visible: this
subsystem records for its own reasons, with its own access path, not because
it shares any code with the glue's recording.

### 4.9 WebView implementation block (`webview_native_activity.h`)

All seven implementation functions share a duplicated JNI attach preamble, the
same accretion pattern the audit code extends; the injected code follows their
comment style (tab-indented `// Audit ...` lines for a maintainer consistent
about it). The seven sites are deliberately not uniform:

* `WebViewCreate` files late — after the attach preamble, so a failure above
  surfaces as a missing entry rather than a stale one;
* `WebViewGetProgress` writes a fold-only increment because it is polled far
  too hot to count;
* `WebViewPostMessage` keys the fold-or-count branch on the `initial`
  parameter — posts of the initial hop count, message returns fold;
* `WebViewRequestRenderToCanvas` counts because canvas draws are exactly what
  page-change reports need;
* `WebViewNativeGetPixels` counts with a comment recording that field support
  changed its mind here;
* `WebViewExecuteJavascript` counts each hop;
* `WebViewGetLastWindowTitle` files a tag-only entry.

The block compiles only under `WEBVIEW_NATIVE_ACTIVITY_IMPLEMENTATION`, i.e.
when one translation unit includes the header as its implementation — which
is how the production application consumes it — so the recording code lives in
the same conditional unit as the code it accompanies.

### 4.10 What was deliberately left out

Two functions were surveyed and left record-*free* intentionally:

* `process_input` — the input-event dispatcher is a hot path; per-event audit
  would flood the record without serving support at all. It is the nearest
  non-participating neighbor of the whole design, and a drag for anyone
  assuming "everything on the pipe records".
* `print_cur_config` — its body is a commented-out debug block; nothing is
  live there to record.

Beyond those, the surrounding application code (the `rawdraw`/CNFG submodules,
`test.c`) cannot participate at this commit because the submodules are not
checked out and the application files are unbuildable here. `ANativeActivity_onCreate`
was left unchanged because creation gets its entries from `android_app_create`
and the app-thread entry frame — a third registration point would repeat a
covered role. The pre-existing functional state fields (`stateSaved`,
`destroyRequested`, `destroyed`, `redrawNeeded`, `activityState`, `running`)
are control flow, not records, and were left strictly alone.

## 5. Behavior-compatibility notes

* every injected statement only assigns, increments or adds to the four new
  `audit*` fields (or reads them inside expressions that write them);
* no existing statement was deleted except four one-line callback bodies that
  were rewritten 1:1 to promote a local and immediately call the same
  function with the same argument;
* the pipe protocol, cond/mutex sequences, saved-state handover, input-queue
  attach/detach, window handoff and UI-thread dispatch are bit-for-bit the
  same logic in the same order;
* the record trades nothing observable: no function's return value, no
  branch, no call sequence is affected by a record read.
