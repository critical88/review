# Injection design record: the broker that absorbed its own pipeline

## Maintenance motivation

ProxyBroker's public entry point is the `Broker` facade in `proxybroker/api.py`:
it owns the user-facing flow — gather proxies from providers or raw data, get
them checked, hand the working ones to the caller's queue, and optionally run
a local serving pool. Historically the "get them checked" part lived in two
dedicated units of the same package: the judge side (`proxybroker/judge.py` —
judge records, their verification, which judges are up per scheme, and random
selection) and the validation side (`proxybroker/checker.py` — the checking
plan plus the per-proxy engine: DNSBL screening, per-protocol connect and
negotiate attempts, test-request building, response decoding, response
validation, and anonymity grading).

The kind of developer pressure that produces the mode recorded here is easy
to reconstruct from real usage: nearly every feature request lands at the
seam between those units. "Use a custom judge list with the default timeout"
needs the facade's configuration when judges are built. "Treat a judge as
down after N seconds" needs the facade's timeout while judges are verified.
"Strict mode should keep only the requested anonymity level" needs the plan
that the facade just assembled. "DNSBL misses should be logged per proxy"
needs the facade's logger and loop. Each request is small, and each fix is
obvious if it is written in the one place where all of that state is already
in scope: the facade class. This record models exactly that evolution as a
completed state rather than as a diff in progress — the point where every
piece of the checking pipeline that the facade touches has quietly become a
facade method, and the two dedicated units are left holding only data.

## Normal code evolution being modeled

The design follows the sequence in which such a class typically accretes:

1. **Judge provisioning moves first.** `find()` must already produce the judge
   objects (it knows the caller's `judges=`, `timeout=`, `verify_ssl=` choices
   before anything else happens). Keeping the default judge list and the
   `urls -> Judge objects` factory next to that call site is the path of least
   resistance: the facade gets a class-level default list and a builder method.
2. **Judge verification follows the factory.** Once the facade builds judges,
   verifying them "right before the run starts" is the next local decision,
   and the shared per-scheme bookkeeping (which judges are available for
   HTTP/HTTPS/SMTP and the events that protocol waiters block on) travels with
   it, becoming facade state because the facade owns the run.
3. **The plan object stays passive while the engine moves.** The facade
   already assembles the checking parameters (types, strictness, DNSBL list,
   real external IP, negotiators), so the natural refactor keeps that object
   as a data carrier — but the engine methods that *use* the plan keep needing
   the facade's timeout, event loop, and resolver, so their bodies are pulled
   into the facade and read the plan via attribute access.
4. **Byte-level helpers follow the engine.** Once the connect/negotiate loop is
   a facade method, its request-builder and response-parsing helpers — which
   used to be small module functions — look like artificial indirection sitting
   in another file while everything they serve is here, so they move too and
   become methods.

The result mirrors what one finds in long-lived Python libraries: a facade
whose body interleaves queue accounting, protocol negotiation, chunked gzip
decoding, and anonymity heuristics in one namespace, with the original
collaborating classes reduced to argument bags.

## Overall design

* `proxybroker/api.py::Broker` is the absorbing owner. It gains:
  * class-level judge state — the default judge list, the per-scheme working
    judge pools, and per-scheme availability events (previously class state of
    the judge unit, previously reset through a `clear()` classmethod);
  * `self._judges_resolver`, a facade-owned resolver instance used for judge
    host resolution and DNSBL reverse lookups;
  * fifteen new methods implementing judge provisioning, judge verification
    and selection, DNSBL screening, the per-protocol check loops, test-request
    building, response decompression, response validation, anonymity grading,
    and strict type filtering;
  * the corresponding imports (`random`, `time`, `zlib`, `aiohttp`, the error
    types, `Judge`, and the wider `utils` helpers).
* `proxybroker/checker.py::Checker` keeps its exact public constructor and all
  derived fields (request-method flag, protocol availability flags, the
  negotiator set, the DNSBL list, plan types) but no longer defines any of
  the validation behavior; the facade consults the plan via attribute access.
  The deprecated `ProxyChecker` subclass is untouched. The five module-level
  helpers of that file (`_request`, `_send_test_request`,
  `_decompress_content`, `_check_test_response`, `_get_anonymity_lvl`) move
  with the engine that calls them.
* `proxybroker/judge.py::Judge` keeps the url-record constructor and derived
  url fields (scheme, host, path, marks), i.e. everything that makes a judge a
 *record*; verification, availability bookkeeping, clearing, selection, and
  the `urls -> judges` factory move to the facade. The module's only remaining
  import surface shrinks accordingly.

Public behavior is pinned by construction: export list, constructor
signatures, method parameter lists of the public facade methods, the queue
protocol, warning texts, and the examples/ and CLI usage patterns are kept
exactly as before the change.

## Per-location rationale

**`proxybroker/api.py` — class-level judge state (`_default_judges`,
`_judge_availability`, `_judge_events`)** — The default judge list is
consumed only by the facade's run setup, so it moves next to that setup. The
per-scheme availability pools and events are the coordination point between
"judges are known good" and "proxy checks may start for protocol X"; in the
new shape the facade owns both ends of that handshake, so the state lives
with it at class level, preserving the previous sharing semantics where all
brokers in one process observed the same judge set.

**`proxybroker/api.py` — `__init__` (`self._judges_resolver`)** — Judge hosts
must be resolved before they can be verified, and DNSBL reverse lookups are
needed per proxy check. One resolver per broker, created where the facade's
other per-run collaborators (queue, semaphore-equivalent `max_conn`, resolver
for proxies) are created.

**`proxybroker/api.py` — `find()` (rewritten body)** — The facade was already
the only place that knows all of `types`, `post`, `strict`, `dnsbl`, the
external IP, and the timeout. The new body resets the process-wide judge
pools per run, assembles the checking plan first, builds the judge set from
its own defaults plus the user's list, and schedules the judge verification
pass as the run's first task. Collecting plan and judges back to back is what
makes the plan feasible to consult via attribute reach-through later.

**`_reset_judges`** — Re-created per `find()` call; mirrors the old
"clear class state before a new run" behavior so availability from a previous
run cannot leak into the next one.

**`_build_judges`** — Factory absorbing the old `get_judges()` module
function: raw urls (or pre-built records) become judge records with the
facade's timeout and SSL policy, defaulting to the facade's own judge list
when the caller passes none.

**`_check_judges`** — The run's verification pass over all judges: resolves
each judge, verifies its page, drops the failures, then applies the
per-scheme consequences — disabling protocols that lost all their judges,
setting their events so waiters do not block, pruning the negotiator set, and
issuing the "Not found judges for ..." user warning. It now mutates the plan
in place (`_req_http_proto` etc.), which is where the plan-as-data decision
becomes visible.

**`_verify_judge`** — Single-judge verification formerly `Judge.check`:
resolve the host, short-circuit SMTP judges, fetch the page with a fresh
request-version header, and accept only status 200 pages that echo the
external IP and the request marker; on success the judge joins the class
availability pool and sets its scheme's event. Positioned on the facade
because it needs the facade's resolver, timeout, and SSL policy at fetch
time — the same parameters the constructor already held.

**`_pick_judge`** — Per-protocol random selection, formerly
`Judge.get_random()`: maps requested protocols to the scheme pool that
serves them (HTTPS/SMTP/HTTP) and draws from the facade's class-level
availability, which is where selection state now lives.

**`_check_proxy`** — The per-proxy validation driver, formerly
`Checker.check`: DNSBL screening first (when configured), waiting on the
per-scheme events for protocols that might lack judges, intersecting the
proxy's expected types with the plan's negotiator set, dispatching to the
SMTP or HTTP-style loop, applying the result and the requested-type filter.
Keeps its old name-signature role but is threaded through `self` so the plan,
resolver, and events are the facade's own state.

**`_in_DNSBL`** — Reverse-ordered lookups against the configured spam
databases, formerly `Checker._in_DNSBL`; now resolves through the facade's
`_judges_resolver` instance rather than a resolver it created itself.

**`_types_passed`** — Strict/non-strict requested-type filter, formerly
`Checker._types_passed`: non-strict accepts any matching protocol at any
level; strict deletes non-matching protocols from the proxy and rejects the
proxy when nothing remains. Reads the plan's requested types and strictness.

**`_check_conn_25`** — The CONNECT:25 (SMTP relay) loop, formerly
`Checker._check_conn_25`: per attempt, set the proxy's negotiator, connect,
negotiate toward the picked judge's host and IP; timeout retries, hard
connection errors stop the loop, success records the protocol with no
anonymity level; the proxy socket is closed in every exit path.

**`_check`** — The HTTP-style per-protocol loop, formerly `Checker._check`:
same retry/abort policy as above, plus sending the test request through the
proxy, decompressing the response, validating the echoed markers, and grading
the anonymity level when the negotiator requires one.

**`_request` / `_send_test_request` / `_decompress_content` /
`_check_test_response` / `_get_anonymity_lvl`** — The byte-level plumbing,
formerly the checker module's five module-level helpers. `_request` builds the
raw request line (with the plan's request method, the judge's host and path,
and the proxy's full-path flag) plus the marker header; `_send_test_request`
pushes it through the proxy socket and returns headers, content, and the
marker, logging success/failure exactly as before; `_decompress_content`
handles gzip/deflate with and without chunked transfer encoding;
`_check_test_response` validates the echoed request-version, referer, and
cookie markers and finds an IP in the page; `_get_anonymity_lvl` compares the
page against the external IP and the judge's own 'via'/'proxy' marker counts
to classify Transparent/Anonymous/High, reading the plan's external IP. These
are placed as facade methods because every remaining caller of them is now a
facade method.

**`proxybroker/checker.py::Checker`** — Keeps the full public constructor and
every derived field so construction-based compatibility is exact: the
request-method flag, the protocol requirement flags, the negotiator set
(pruned later by the facade's judge pass), the DNSBL list, the requested
types, strictness, and the external IP. All engine methods and the module
helpers are removed with the engine. What remains is the checking *plan* the
facade assembles and consults; the class docstring says so explicitly.

**`proxybroker/judge.py::Judge`** — Keeps the record identity of a judge:
parsed url fields, marks, timeout and SSL fields, and the constructor that
derives them. Availability class state, `clear()`, `get_random()`, `check()`
and the `get_judges()` factory are removed with the rest of the judge
management; the module's `aiohttp`/resolver/logging imports go with them. The
class remains fully constructible and importable with the same signature.
