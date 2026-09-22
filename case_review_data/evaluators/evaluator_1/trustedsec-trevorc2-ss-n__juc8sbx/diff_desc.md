# Injection design record — TrevorC2 C agent (`trustedsec-trevorc2-ss-n`)

Repository: https://github.com/trustedsec/trevorc2, pinned at
`573a1ee7787448a9fa57085c4c6c5a566d3e60b8`. Target of this work: the C implant
under `agents/c/`.

## The system being modeled

The C implant is a small beacon. On startup it performs one initial HTTP
check-in (`connectTrevor`), learns the session cookie from the response, then
loops forever: poll for tasking (`getTasking`), shell out to the tasking command
(`doTasking`), and deliver results (`sendTasking`). Polling and results traffic
is encrypted with a pre-shared secret through `encrypt_buffer` /
`decrypt_buffer`, and everything moves over a cover URL through the HTTP
transport `http_request` in `agents/c/posix_src/http.c`.

The whole channel the beacon talks on is specified in one place,
`agents/c/include/tc2_config.h`: the operator's server address, the wire port,
the cover URL pieces (`ROOT_PATH_QUERY` for polling, `SITE_PATH_QUERY` plus
`QUERY_STRING` for check-in and results), the session cookie name
(`COOKIE_VALUE`), the pre-shared AES key (`AES_KEY`), the cover markers
(`STUB`/`ENDSTUB`), and the polling intervals. Every stage reads its pieces
from that header. That file, plus the transport templates, is effectively the
only place an operator touches to stand the implant up against a new channel.

## Maintenance motivation

The realistic operational pressure on this code is channel churn. Operators
re-point the implant constantly: new redirector addresses, moved ports, rotated
cover paths, renamed query parameters after blue-team fingerprinting, rotated
pre-shared secrets. Under that pressure, the natural instinct of a maintainer
responsible for *one stage* is to stop depending on the shared header for the
values that stage is allowed to break on: the header is co-edited by everyone,
it changes underneath you mid-engagement, and your stage silently starts
talking to the wrong channel. Pinning the values your stage needs reads as a
legitimate hardening move at the time — every one of these edits ships as an
individually defensible ops patch.

## Normal development evolution being modeled

The diff models a sequence of small, independently survivable patches, not one
engineered refactor:

1. A registration hotfix: the initial check-in kept dying during header churn,
   so the check-in stage pinned its own address, port, and the cookie tag it
   parses the response with.
2. A polling tweak: the poller's maintainer stopped reading the address from
   the header and routed it through a local hook that "always returns the live
   one", plus pinned the poll port.
3. A results-channel change: the results stage moved to a validated endpoint
   record, pinning its own host, port, cover path, and query key in the
   process.
4. A transport-resilience pass: the socket layer learned to fall back to its
   own copy of the address when a caller hands it an empty host, and took over
   rendering the cookie header itself with its own cookie name.
5. An interop pin: while testing fresh tasking payloads, the decrypt side
   pinned its own copy of the pre-shared secret so it stopped following the
   header that the encrypt side (and the operator's server) still follows.

Each patch left the build green and the wire traffic identical, because each
pinned value was copied verbatim from the header it stopped reading. That is
exactly why each one is survivable in review: nothing about the beacon's
observable behavior changes on the day the patch lands.

## Overall design

The design spreads knowledge of *how to reach the channel* across the stages
that use it, in the shapes a C maintainer would actually reach for:

- **Scalar stage-local pins** (`static` file-scope-in-function char arrays and
  an int) for values a single stage needs — the C idiom for "constant, but
  mine".
- **A composite record** (`struct beacon_endpoint`) for the results stage,
  where address and port travel together.
- **A value-yielding accessor** (`tasking_channel_host`) for the poller, whose
  maintainer wanted a seam "to swap the live address" — the body is the
  constant itself.
- **Transport-level re-binding with composition**, where the socket layer both
  holds its own fallback address and re-renders the cookie header from its own
  cookie-name binding.
- **A crypto-side secret pin**, the classic two-sides-of-one-protocol split.

Behavior is preserved everywhere: every pinned value is a verbatim copy of the
header value it shadows, the request templates keep their exact bytes, and the
HTTP operands assemble to the same wire requests. The header still exists and
still owns the values that were *not* re-bound — timing, cover markers, the
encrypt-side secret — so the file remains the place where a channel-wide change
*starts*, even though it no longer reaches every stage that needs it.

## Per-location rationale

### `agents/c/src/main.c` — file scope (registration/polling support types)

Added a `struct beacon_endpoint` record type and the `tasking_channel_host`
accessor at file scope. The record gives the results stage a place to keep
address and port as one validated unit; the accessor gives the poller its
"live address" seam. Both are file-level because stages in this file-share one
translation unit; in a multi-file agent these would have been stage headers of
their own.

### `agents/c/src/main.c` — `connectTrevor` (initial check-in)

Pinned `REGISTRATION_HOST`, `REGISTRATION_PORT`, and `SESSION_COOKIE_TAG`
inside the check-in, then switched the call to `http_request` and the
`strstr`/`strlen` cookie parsing onto those bindings. Site chosen because it is
the stage that tolerates the least channel ambiguity: it must reach the right
server before a session exists, and it learns the session from a response
whose cookie name it hard-parses. The production role is registration — the
one-shot bootstrap whose failure mode is "beacon never checks in".

### `agents/c/src/main.c` — `getTasking` (polling)

Pinned `TASKING_PORT` and routed the host argument through
`tasking_channel_host()`. The poller is the stage a repointed channel breaks
most visibly (missed tasking), which is precisely why its maintainer would
insulate it from header churn. The port kept the uint16_t the transport
signature already implies.

### `agents/c/src/main.c` — `sendTasking` (results)

Pinned `RESULTS_CHANNEL` (endpoint record), `RESULT_PATH`, and
`RESULT_QUERY_KEY`, and rebuilt the results URL and request operands from them.
The results stage is where a channel-split operator story is most credible:
exfil traffic often moves to dedicated infrastructure. The cover URL had to be
recomposable because the value feeds `sprintf` composition — so this site pins
each URL piece separately (path and query key), which is how a C maintainer
would hold "my own cover URL" without a URL abstraction.

### `agents/c/posix_src/http.c` — `http_request` (transport)

Pinned `DEFAULT_CHANNEL_HOST` as the transport's own fallback for an empty
`hostname` argument, pinned `SESSION_COOKIE_NAME`, and moved cookie-header
rendering into the transport: the `REQUEST_TEMPLATE` cookie operand changed
from `Cookie: sessionid=%s` to `Cookie: %s`, with the pinned name joined to
the session value in `http_request` before rendering. Site chosen because it
is the layer that actually formats the wire request: with every stage free to
pass (or not pass) its own host, the transport learns "its own" notion of the
channel. The fallback composition preserves the original case where callers
always passed a host, and the header change keeps the rendered request
byte-identical.

### `agents/c/src/crypto.c` — `decrypt_buffer` (payload crypto)

Pinned `TASKING_SESSION_SECRET` on the decrypt side only.
`encrypt_buffer` deliberately keeps its header reference: the split models a
key-rotation half-finished, the way protocol mismatches actually happen. The
production role is tasking confidentiality — and the asymmetric pin means the
two directions can now disagree about which secret is current.

## What was deliberately left alone

The vendored AES/SHA/base64 primitives (`aes.c`, `sha256.c`, `encode.c`,
`decode.c`) carry no channel knowledge and got none. The request templates'
bytes (User-Agent, header order, connection semantics) stay identical, since
matching operator expectations for wire traffic is part of the deployment
contract, as do the `STUB`/`ENDSTUB` cover markers, the polling intervals, and
the encrypt-side key. `agents/c/src/main.c`'s `doTasking` and the shell
execution path were left untouched: they never touch the channel identity.
