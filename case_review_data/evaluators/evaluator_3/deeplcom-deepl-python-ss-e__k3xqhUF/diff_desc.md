# Injection design record — scattered response-status interpretation in the DeepL clients

## Motivation

The client library's error contract — which `deepl` exception a user gets for
a given HTTP status code, with which message text, retry hint, and status-code
attribute — used to live in one place: a private helper on the translation
client that every endpoint method called with the response triplet
(status code, content, parsed JSON). Two enum-like boolean flags on that
helper had accumulated to cover the two operations that needed different
semantics: `glossary=True` for the glossary lifecycle endpoints (404 must
raise the glossary-specific not-found exception rather than the generic one)
and `downloading_document=True` for the translated-document download (503
must raise the retryable document-not-ready exception rather than generic
service-unavailable).

That helper signature was a recurring source of friction in review
discussions: boolean flags on a policy function invite call sites that
boolean-paste their way to whatever policy they want, and the two semantics
it special-cased were operation properties, not response properties. The
modeled development removes the flags and pushes the special cases back out to
the operations that need them, letting each operation intercept exactly the
status codes that matter to its users before handing everything else to the
shared fallback. The second client generation (rephrase/correct, multilingual
glossaries, style rules, translation memories) then builds on that pattern:
hot endpoints get their own overload handling, resource-lookup endpoints get
their own not-found handling, and the pre-signed file-transfer endpoints —
which never had a use for DeepL-authorization semantics anyway — stop
sharing a helper entirely and inline their own stripped-down policy, after
which the transfer-specific helper is deleted because nothing else calls it.

## Modeled evolution

The change models one extended, plausible period of incremental work:

1. **Flag dissolution.** The two mode flags disappear from the shared helper.
   Their behavior does not disappear; it moves into the affected operations.
2. **Hot-path specialization.** Operations with the worst blast radius when
   the API is saturated (text translation, document upload, the rephrase and
   correct write-back paths) gain local handling for the overload statuses so
   that the retry behavior and message wording on those paths are owned next
   to the request that produces them. The usage and language-listing calls,
   which integrations issue at startup, gain local handling for the
   authorization and quota statuses for the same reason.
3. **Resource-lookup specialization.** Glossary endpoints — both API
   generations — style-rule endpoints, and translation-memory resource
   endpoints keep their not-found decisions locally, because a lookup that
   misses is the salient failure of those endpoints, and which specific
   exception a 404 must produce is a property of the resource being looked up.
4. **Deletion of the orphaned transfer helper.** With the two pre-signed
   transfer endpoints now carrying their own policy, the transfer-helper is
   no longer called by anyone and is removed.

The migration is deliberately *incomplete*: the shared fallback remains and
still carries the generic policy (unauthorized, quota exceeded, generic
not-found, bad request, rate limited, generic service-unavailable, and the
unexpected-status breakout), and several endpoints — collection listings, job
polling, aggregate-creation endpoints whose salient failure status is not
404 — continue to delegate entirely. That is exactly the shape of an
in-flight migration: policy fragments accumulate at the sites people were
actually touching.

## Overall design of the injected state

Every fragment follows the same protocol but not the same shape: after the
request call returns the status code, the operation checks the statuses it
owns locally, raising the same exception (type, message including any
server-provided suffix, `should_retry`, `http_status_code`) that the caller
would previously have received, and delegates every other status to the
shared fallback with the response triplet. Two exception imports are added to
the second client module because it now raises exception types it previously
only triggered indirectly.

Because multiple contributors are being modeled, the fragments were
intentionally **not templated**. They differ in:

- **assembly placement** — some build the `, message: `/`, detail: ` suffix
  up front before touching the status code; others build it inside the
  branch that raises; one builds it with a small local loop over
  `(key, label)` pairs;
- **comparison style** — `http.HTTPStatus.NOT_FOUND` enum members, bare
  `404` literals, and a membership test against a tuple of the two overload
  statuses;
- **control shape** — separate single-status `if` statements, an
  `if`/`elif` pair, a nested two-stage branch for the overload statuses, and
  an inverted form that delegates first and raises in the `else`;
- **local naming** — `error_detail`, `load_detail`, `throttle_detail`,
  `missing_glossary`, `glossary_reason`, `lookup_failure`, `not_found_note`,
  `absent_note`, and so on.

This variation is what a fragment population looks like after several people
have each solved the same small problem at their own call site, and it means
the suffix-assembly logic no longer has a single authoritative spelling.

## Changed locations and rationale

### Shared fallback — `Translator._raise_for_status` (base client module)

The method loses its two mode parameters and the special cases they chose
(the glossary 404 and the document-not-ready 503), keeping the generic policy
for the statuses nobody has localized: the early success return, the
authorization, quota-exceeded, generic not-found, bad-request, rate-limit and
generic-service-unavailable branches, and the unexpected-status breakout that
formats the raw content. Removing flags here is the enabling decision for
everything else: operations can no longer select special semantics through a
shared switch, so each operation that needs them states them itself.

### Overloading text and write-back paths — 4 methods

`translate_text`, `translate_document_upload` (first client), `rephrase_text`
and `correct_text` (second client) each gain local rate-limit and/or
service-unavailable handling next to their request. These are the paths users
put behind user-facing wait states, so the retryable-exception decisions
(code 429 and code 503 with `should_retry=True`) were the ones teams kept
patching locally. The text-translation fragment checks each overload status
separately; the document-upload fragment uses one membership test and then
splits; the rephrase fragment mirrors the translation one (the two APIs
share output shape, so the same wording is wanted); the correct fragment is
slimmer and only names the rate-limit status. All four still delegate, so the
other statuses keep their shared behavior and message syntax.

### Account and discovery calls — 2 methods

`get_usage` and `get_source_languages` (first client) gain local handling for
the authorization failure, and the usage call additionally for the
quota-exceeded status. These are the calls integrations make at startup to
validate a key and to populate language pickers; having the authorization
and quota outcomes owned next to them was the requested ergonomics change.
The usage fragment chains the two checks with `elif`; the language-listing
fragment checks a single status.

### Glossary lifecycle, first generation — 5 methods

The private glossary-creation helper plus get/list/entries/delete gain a
local 404 check that raises the glossary-specific not-found exception with
the server-suffix assembly inline, then delegate. This is where the
glossary=True flag used to do its work, so these five sites explain why the
flag could be removed without losing the user-visible distinction between
"your glossary id is not a glossary at all" (delegated, generic) and "no
glossary with that id exists" (local, glossary-specific). The shapes differ
per site (pre-assembled versus in-branch suffix, enum versus literal
comparison, one loop-built suffix, and one inverted delegate-first form) to
match the multi-author story.

### Multilingual glossaries, second generation — 9 methods

The glossary creation, name update, dictionary update/replace, get, list,
entries, delete and dictionary-delete operations gain the same local
404-to-glossary-not-found interception. The v3 schema has the same product
requirement as v2 — a missing glossary is reported as the glossary not-found
exception — so the same local decisions appear here. Nine functions carry
it because every operation in that feature addresses a specific glossary or
glossary-dictionary resource. Two of them deliberately use the inverted
(delegate-first) form and two use suffix loops; the rest pre-assemble.

### Document workflow — 2 methods

The document-status polling method gains a local generic 404 branch (clients
polling a deleted or mistyped document id want the plain not-found message),
and the document-download method gains the local 503-to-document-not-ready
mapping — the direct replacement for the `downloading_document` flag. The
download fragment keeps delegating all other statuses, including the raw
"<file>" content placeholder the shared fallback formats when an
unexpected status arrives on a streamed body.

### Style rules and translation-memory resources — 4 methods

The get and delete methods for style rules and for translation memories gain
a local generic 404 branch each. These four endpoints take a caller-supplied
identifier for a stored resource, so a missing entry is their salient
failure; the surrounding list/creation/polling operations continue to
delegate because a 404 is not the meaningful failure mode for them. Two of
the four use the inverted form, one uses a literal comparison.

### Pre-signed storage transfers — 2 methods and one deletion

The translation-memory file upload and export-download methods speak to
pre-signed storage URLs, not to the DeepL API: no authorization header, no
parsed JSON, and a deliberately plain failure envelope ("Error <action>,
HTTP status: <n>[, detail: ...], never retry). In the modeled history a
previous refactor had already given these two a tiny shared helper of their
own; with the response-policy now being localized per endpoint, the last
two callers stopped delegating and the helper was (deleted as) dead code.
Both methods now hold their own copy of the transfer policy, one expressing
the success range as a negated range check and the other as two explicit
bounds — the two spellings another sign of separate authors. The upload
fragment reads the response body detail only when the content is a string;
the download fragment mirrors it for the streamed case.

### Imports

The second client module imports the rate-limit and glossary not-found
exception constructors it now invokes directly, and the `http` status enum.
The first client module already had every name it needs.

### What was deliberately left alone

The transport layer's retry/backoff timer still consumes status codes for
its own purpose (whether to repeat a request), but never raises the
library's exceptions, and does not change here. The data-model constructors
still interpret response JSON. Endpoint parameter validation still raises
`ValueError` before any request happens. A content-format check that raises
after a successful response in glossary entry retrieval is gated on the
payload shape, not on the status code, and remains as it was. The endpoints
that never localized a status in this story still call the shared fallback
with the response triplet only.
