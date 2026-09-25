# Injection design record — salarcode/SmartProxy subscription pipeline (task `salarcode-smartproxy-dc-n`)

This document records why the change in `smell.diff` exists: the maintenance
situation that motivated it, the normal development evolution it models, the
overall shape of the change, and a per-location rationale for every materially
changed cluster. It is written as an auditable design narrative, not as a
verdict on the code.

## Maintenance motivation

SmartProxy has shipped two subscription features for a long time:

* **Proxy-server subscriptions** (`ProxyImporter`, since 2019): periodically
  download a list of proxy servers, parse it as plain text or JSON, and keep
  the servers in settings.
* **Proxy-rule subscriptions** (`RuleImporter`, since 2022): periodically
  download rule lists (AutoProxy/GFWList or SwitchyOmega formats) and compile
  them into internal proxy rules.

Both features perform the *same* network dance: mark the URL as a
"special request" so the request honors the subscription's own proxy routing,
optionally build a Basic-auth header, fetch the remote text, and only then
hand the body to the parser. In the pre-change code that dance is duplicated
verbatim inside `ProxyImporter.readFromServer` and
`RuleImporter.readFromServerAndImport`, including the subtle bits: the
`atob(password)` before `btoa(...)` for the Authorization header, and the
special-URL registration guarded on `applyProxy !== null`.

The motivating maintenance task is the classic one a maintainer faces when
this duplication starts to bite: a fix to one copy (auth handling, status-code
handling, retry) keeps missing the other copy. The diff models the first
increment of the natural repair: **extract the shared transport so both
subscription kinds fetch through one place** — and, as so often happens in
real code, do it in a hurry, threading the request parameters through the new
boundary one field at a time instead of moving a cohesive value.

## The developer evolution being modeled

The change is a single understandable maintenance increment, not a synthetic
mash. In story form:

1. Transport extraction. A new `SubscriptionFetch` helper is introduced as the
   one place that marks special requests, builds the optional auth header and
   performs the fetch. Both readers are rewired onto it. The transport hand
   back `(responseText, statusCode, statusText)` so each reader can keep its
   own success/fail policy (the proxy reader historically fails on non-200
   with `` `${status}, ${statusText}` ``; the rules reader is stricter about
   empty documents).
2. Signature threading. Rather than changing the readers to accept the stored
   subscription (or a request value built from it), each connection aspect is
   spelled out in the reader signatures: `url`, `username`, `password`,
   `obfuscation`, `format`, `applyProxy` — plus the server-side `proxyProtocol`
   where it matters. The maintainer wants the readers call-controllable, and
   the fields are already named constants in his head, so they become a row of
   parameters.
3. Parse plumbing. The parse step needs the `obfuscation` and `format` that
   describe the downloaded body, so those two travel on — as optional trailing
   slots on the proxy importer, as leading slots on the rules importer, and
   from there into `doImport`, `parseText`, and `parseJson`.
4. Call-site updates. Every caller of the two readers is updated to press its
   stored subscription into the new field rows: the scheduled refresh timers in
   `SubscriptionUpdater`, the settings-page save/test/refresh handlers, and
   the two import modals.

This is exactly how parameter lists grow in production code: a shared helper
is born with a flat signature "for caller control", callers forward field
after field because that is what the compiler asks for, and two years later
the same six names are typed out in a dozen places. Nothing in the story
requires bad intent or unusual discipline failure; each step is locally
reasonable.

## Overall design of the change

* **New module** `src/lib/SubscriptionFetch.ts` — the shared download
  transport, as a small class with one static entry point, matching the
  codebase idiom of single-purpose static helper classes
  (`SubscriptionStats`, `ProxyEngineSpecialRequests`).
* **Two readers**, `ProxyImporter.readFromServer` and
  `RuleImporter.readFromServerAndImport`, keep their responsibilities (their
  own validation, callback contracts, and parse dispatch) but now receive
  every connection aspect as individual parameters and delegate the wire work
  to the transport.
* **Two importers**, `ProxyImporter.importText` and
  `RuleImporter.importRulesBatch`, gain the body-describing members
  (`obfuscation`, `format`, `proxyProtocol`) so parsing can decode and
  dispatch locally; the rules batch importer drops its UI-config-object
  parameter in favor of the two plain values that were ever needed.
* **Call sites** in `SubscriptionUpdater` (timer-driven refresh of both
  subscription kinds) and `settingsPage` (save, test and grid-refresh
  handlers) forward their stored subscription/model fields positionally into
  the new reader signatures.

No tests were modified; no behavior is intended to change. All files touched
are production sources in the subscription/import subsystem.

## Per-location rationale

### `src/lib/SubscriptionFetch.ts` (new file)

*What changed:* a new class whose `fetchSubscriptionText(url, username,
password, obfuscation, format, applyProxy, success, fail)` performs the GET:
registers the special URL when `applyProxy !== null`, decodes the password and
builds the Basic-auth header only when `username` is present, fetches with
`cache: 'no-store'`, and returns the body with `statusCode`/`statusText` to
the mandatory success callback; failures go to the optional fail callback.

*Why this location and form:* both readers previously carried byte-identical
copies of this dance, so the transport belongs in the shared `src/lib`
namespace next to its callers; a class keeps the name callable as
`SubscriptionFetch.fetchSubscriptionText` the same way sibling helpers call
each other in this codebase. The signature names every request aspect
instead of accepting a subscription value: the transport is meant to be
usable from any caller (not just subscription objects), and each parameter
documents exactly what the wire layer consumes.

*Why `obfuscation` and `format` are carried at all:* the original intent
(really, the second paragraph of its doc comment) was to move the decode step
into the transport later, so the connection parameters were mirrored ahead of
time and then simply forgotten in the signature. They are received, never
read — the parse still happens in the readers. Their presence documents how
the caller got the body; their silence documents how such lists evolve.

*Production role:* single choke point for subscription downloads. Any future
fix to auth, retries, or special-request handling lands here.

### `src/lib/ProxyImporter.ts` — `readFromServer`

*What changed:* the signature grew from `(serverDetail, success?, fail?)` to
`(url, username, password, obfuscation, format, applyProxy, proxyProtocol,
success?, fail?)`. The body keeps the reader's own guards (`!url` short-fail,
mandatory success callback, the empty-body quirk that logs a failure and
continues into the import path), gates on `statusCode === 200`, and passes the
parse-describing tail into `importText`.

*Why this form:* the maintainer wanted the reader call-controllable — explicit
parameters mean a caller can fetch a list with a one-off protocol override or
a temporary credential without mutating a stored subscription object first.
That convenience is exactly why the full field row appears here and in the
rules reader with near-identical spelling.

*Production role:* entry point for server-subscription download; own callback
contract `{success, message, result: ProxyServer[]}` unchanged.

### `src/lib/ProxyImporter.ts` — `importText` / `doImport` / `parseText` / `parseJson`

*What changed:* `importText` gained trailing optional
`obfuscation?, format?, proxyProtocol?` parameters; the inner `doImport`
receives them explicitly and dispatches JSON vs plain text using the format;
`parseText` now decodes base64 obfuscation itself and applies the per-line
protocol default (`HTTP` unless `proxyProtocol` was given); `parseJson` decodes
the same way. The trailing position and optionality keep the two existing
local import call sites working unmodified.

*Why this location:* the importer is where the body shape becomes relevant, so
the decode moved here from nowhere (upstream, server text lists were parsed
wholesale via the stored subscription). Trailing optional slots were the
least invasive choice at the time.

*Production role:* converts downloaded (or locally selected) proxy lists into
`ProxyServer[]`, deduplicating and reporting counts exactly as before.

### `src/lib/RuleImporter.ts` — `readFromServerAndImport`

*What changed:* mirror of the server reader: the signature now enumerates
`(url, username, password, obfuscation, format, applyProxy, success?, fail?)`
and the fetch/username/password/special-URL block is replaced by a call into
`SubscriptionFetch`. `ajaxSuccess` retains the strict empty-document check and
forwards into `importRulesBatch`.

*Why this location and form:* same motivation as the server reader; the rules
reader differs in tail (no proxy protocol concept) and in strictness (must not
produce an empty rules container from a blank body).

*Production role:* entry point for rule-subscription download+import.

### `src/lib/RuleImporter.ts` — `importRulesBatch` / `doImport`

*What changed:* the importer previously took a rules-config object
(`IExternalRulesConfig`-shaped) and read `obfuscation`/`format` off it; the
signature now leads with plain `obfuscation, format` and `doImport` receives
them directly. Base64 decode, AutoProxy (detect/parse) and SwitchyOmega
(parse/compile/convert) dispatch all stay in `doImport`, as does the
deduplication contract and the message formatting for both dedup modes.

*Why this form:* with the config object carrying only two consumed fields, the
maintainer "simplified" the batch importer to the values themselves — a common
and locally defensible move that also makes the value-row spelling identical
to the other signatures.

*Production role:* the bulk importer for rule lists, invoked from network
reads and from the local file/text modals alike.

### `src/core/SubscriptionUpdater.ts` — `readServerSubscription` / `readRulesSubscription`

*What changed:* both timer-driven refresh paths now press the freshly found
`subscription` object into the new field rows: `subscription.url`,
`subscription.username`, `subscription.password`, `subscription.obfuscation`,
`subscription.format`, `subscription.applyProxy`, and the server side also
`subscription.proxyProtocol`. Callback bodies (stats updates, persistence,
proxy/rules change notification) are untouched.

*Why this location:* these two methods are the auto-refresh heartbeat for both
subscription kinds; they must serve whatever the picker found in settings —
hence field-by-field extraction instead of relying on the model type. Only the
fields that go down the wire are named, which documents the contract between
storage and network precisely.

*Production role:* scheduled lifecycle for subscriptions (`setInterval`/
`setTimeout` entries elsewhere in the same file reference these methods).

### `src/ui/code/settingsPage.ts` — subscription save/test/refresh handlers

*What changed:* five call sites follow the same pattern:
`onClickSaveServerSubscription` (save & fetch server subscription),
`onClickTestServerSubscription` (test button; the call happens inside the
special-request round-trip), `onRulesSubscriptionRefreshClick` (grid refresh),
`onClickSaveRulesSubscription` (save), and `onClickTestRulesSubscription`
(test; also inside the special-request round-trip). Each forwards
`subscriptionModel.url … subscriptionModel.applyProxy` (server handlers add
`proxyProtocol`) into the readers. The two test handlers keep their legacy
two-step special-request dance, including clearing `applyProxy` on the model
before the reader runs so the transport does not re-register the same URL.

*Why these locations and this form:* the settings page owns the editor-bound
model (`subscriptionModel`/`editingSubscription`) built from the form. The
handlers already contained the two-step test flow and the stats/persistence
side effects; positioning the field extraction directly inside each handler
keeps each button self-contained and readable, which is precisely why the
same field row gets typed out five times in this file.

*Production role:* the management UI for subscriptions in the extension's
options page.

### `src/ui/code/settingsPage.ts` — rules import modal (`onClickImportRules`)

*What changed:* the modal used to build a `ProxyRulesImportFromUI` object and
hand it to the batch importer; it now passes `null` for the (unused) network
obfuscation and the selected `sourceType` directly, because
`importRulesBatch` no longer wants the object form. Nothing else in the modal
flow changed (file/text selection, append/replace choice, validity warning
for unsupported source types).

*Why this location:* the modal is the local (non-network) caller of the same
importer the rules reader uses; accepting the signature change here is
required for the modal to keep importing local files at all.

*Production role:* local rules import in the options page.

## Structural variation deliberately present in the change

* The field row appears with **different tails and positions** per owner: the
  server reader ends with `proxyProtocol` and callbacks; the rules reader with
  callbacks only; the batch importer leads with `obfuscation, format`; the
  text importer trails with all three optional extras.
* Two members of the row (`obfuscation`, `format`) are **inert** in the
  transport: a caller must still pass them, yet the transport never consults
  them.
* The row mixes **required and optional** slots (`success?`, `fail?`,
  `obfuscation?`), so no single textual pattern describes all occurrences.
* Pre-existing recurring parameter trios elsewhere in the codebase — for
  example the `(message, sender, sendResponse)` browser-message callbacks and
  the insecure `(success, fail)` pairs everywhere — were left untouched; they
  are part of the environment the maintainer works in, not part of this task.
* The injected spelling coexists with object-based plumbing that was already
  there: settings persistence, `CopyFrom`, and the subscription grid all move
  whole subscription objects without ever taking them apart.

## Behavioral notes (facts relied on while writing the change)

* `atob(password)` still runs synchronously before `fetch`, and only when a
  username exists, so invalid base64 fails fast exactly as it did.
* Special-URL registration is still guarded on `applyProxy !== null`, and the
  test flows still pre-register then clear the mode before the reader runs.
* The readers still gate parsing on `statusCode === 200` (server) and still
  refuse empty documents (rules), including the server reader's historical
  continue-after-empty-body path.
* Parsers still perform their base64 decode, JSON/plain-text dispatch,
  per-line protocol defaulting, deduplication, and success-message
  interpolation unchanged; the local proxy import modal still imports plain
  text with no decode extras.
