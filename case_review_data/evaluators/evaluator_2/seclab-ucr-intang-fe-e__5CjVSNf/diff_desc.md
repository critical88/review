# Injection design record — per-connection activity ledger in INTANG

## Maintenance motivation

INTANG measures how different censorship-resistance strategies perform. The
in-memory cache module already tracks coarse per-connection facts (which
strategy was dispatched) and per-host facts (historical results, server TTLs),
but nothing answers the operational question the authors cared about: *for a
single connection, what actually happened during its treatment cycle* — which
strategy was tried and how many have been tried, whether the client's request
and the server's response were observed, whether the censor answered with a
type-1 or type-2 reset, which discrepancies were applied to the flow, and what
TTL hint the insertion packets used. The TODO notes in the cache module ask for
exactly this: over time, know per connection and per host which discrepancies
are applicable and which treatments actually work.

The change adds a *per-connection activity ledger*: one record per connection
keyed by the connection 4-tuple, recording the evidence of the current
treatment cycle, plus an operational summary log (`[LEDGER] ...`) so a
deployment can read what a cycle looked like when it closed.

## The evolution being modeled

This is the normal first landing of such a feature. The cache module ships the
record type, its storage and a handful of read accessors first, because that is
the part that needs one owner: the record layout, the hashed key, the bounded
store (the module already keeps similar bounded lists and prints a warning when
full), and simple getters for the fields an operator asks about. Then every
call site wires itself up where its observation happens: the pipeline sees a
SYN and provisions a cycle; a request leg marks its evidence; a response leg
marks its own; the RST classifier marks attack evidence; the evaluator closes
the cycle; the insertion path records what it sent; the prober corrects the TTL
estimate; the strategies report the treatments they fire. Each consumer knows
its own observation, so each consumer updates the record itself.

That is also where the maintenance pain comes from, and the reason the change
is worth reviewing as a unit: the record's rules — what "seen", "reset
evidence", "cycle closed", "TTL hint refined" mean — are assembled from the
consumer functions rather than by the module that defined the record. The
module's header comment states the intended discipline ("other modules should
consult it through the accessors below instead of reaching into the record"),
and the feature grew past that discipline the way features do.

## Overall design

* `src/memcache.h` defines `struct conn_ledger` — the record, the
  `LEDGER_EV_*` evidence taxonomy for its `events` field, and a small accessor
  API (`ledger_ref`/`ledger_open` plus three read-only getters). The record
  embeds the connection 4-tuple as its key, mirroring how the module's other
  caches are keyed.
* `src/memcache.c` implements the storage: creation into a list plus a hash
  bucket (following the existing `create_conn_info_entry` pattern), hashed
  lookup by all four key fields, open-or-fetch with the store refusing new
  records past the module's existing size bound, and the read accessors.
* The consumer side of the daemon is where accounting is maintained inline:
  six daemon paths and both self-reporting strategies open or look up the
  record and update its fields directly, each according to what it just
  observed.
* A debug-summary helper mirrors the module's existing cache-summary helpers,
  and a reporting helper prints a connection's ledger line through the read
  accessors only.

## Per-location rationale

### `src/memcache.h` — record, taxonomy, API

New include of `time.h` and of the protocol header (the record needs the
4-tuple type). The `LEDGER_EV_*` constants are defined next to the record so
the evidence vocabulary has one home; each bit is documented with its meaning
(seen / request / response / type-1 reset / type-2 reset / cycle verified /
insertion applied). `struct conn_ledger` carries the key 4-tuple and the
cycle facts: dispatched strategy id, attempt count, verdict, evidence bitset,
applied discrepancy codes, the last observed mark (sequence number, transaction
id or evidence value), the server-side TTL estimate used by insertion packets,
a timestamp, and list links. The header declares lookup, open, and three
field getters, placed with the record so consumers have an accessor surface to
use. The block comment records the intent and the cache TODO it serves.

### `src/memcache.c` — storage behind the API

A `struct ledger_ht_node` hash-chain node, the static list head and length,
and the shared hash table back the record. `create_ledger_entry` mirrors the
module's existing `create_conn_info_entry` (zero the record, copy the key,
timestamp, push onto the list, insert into the hash bucket). `ledger_ref` is a
four-field key comparison over the bucket chain; `ledger_open` fetches and
creates, preserving the module's bounded-store convention — refuse past the
existing size cap and print the same style of warning the neighboring caches
print. The getters (`ledger_sid`, `ledger_events`, `ledger_ttl_hint`) are
null-tolerant reads used by reporting code. A `printf` summary helper mirrors
`conn_info_cache_summary` for parity with the module's other stores.

The storage deliberately reuses the module's own idioms — existing hash
function, size bound, list pattern — so the record keeps proven cache
mechanics.

### `src/main.c` — the packet pipeline

`process_tcp_packet` handles the whole TCP side, and each phase of the
connection lifecycle passes through it, so the cycle accounting lands in the
phase branches where observations occur:

* On outgoing SYN, after a strategy is chosen for the connection, a cycle
  record is opened and populated inline: the strategy id, a bumped attempt
  count, the host TTL as the initial hint, the "seen" evidence bit — and,
  when several strategies have already been consumed, the retry notice in the
  summary log.
* On the HTTP request leg and the DNS-over-TCP request leg, the request
  evidence bit is marked along with the triggering sequence number.
* On the SYN-ACK leg (the connection is alive), the response evidence bit is
  marked and, if no hint was adopted yet, the server-side initial TTL is
  computed and adopted.
* On the incoming HTTP response leg, the response evidence bit is marked with
  the observed sequence number.
* In the fallback branch for other incoming protocols, the reporting helper
  prints the connection's ledger line — this consumer reads only, through the
  getters.

`process_udp_packet` covers the UDP DNS shadow-resolution legs: the redirected
request leg and the completing response leg each mark their evidence with the
DNS transaction id, matching how the module's DNS caches are keyed by that
id.

### `src/cache.c` — attack classification and evaluation

`_process_incoming_RST` classifies reset attacks; the two classification legs
add their evidence inline: the type-1 leg marks type-1 reset evidence, the
type-2 leg marks type-2 reset evidence and records the observed attack TTL as
the mark.

`process_timeout` runs when an evaluation cycle ends without further traffic;
both of its verification branches (the HTTP-based and the DNS-based) close the
cycle by interpreting the record inline: snapshot the evidence bits, derive a
verdict (reset attack versus responded versus nothing), adapt the TTL hint
within its documented bounds (step up on reset evidence, step down when
nothing was heard), mark the cycle verified, and emit the cycle summary log
with the connection 4-tuple, verdict, evidence bits and hint. The two
branches are mirrored on purpose — the sweep treats both kinds of connections
with the same close-out discipline.

### `src/discrepancy.c` — the insertion choke point

Every crafted packet the daemon sends — fake SYN, FIN, RST, data, desync —
goes through `send_insertion_packet`. That makes it the one place that knows
which discrepancies a connection's cycle actually used, so the journaling
sits at its top: the tuple is rebuilt from the send variables (ports are
converted with `htons` because the send variables are host-order while the
ledger key follows the pipeline's convention), a record is opened, and the
insertion evidence bit plus the low bits of the applied discrepancy codes are
written.

### `src/ttl_probing.c` — confirmed server TTLs

`process_synack_for_ttl_probing` sees a response that confirms a probed TTL.
The hint refinement uses the same unset-or-tighter rule as the per-host TTL
map: adopt the confirmed TTL when no hint exists yet or when it is smaller.
The tuple is rebuilt from the probing perspective with `htons` on the ports,
since the probing ports are derived host-order values.

### `src/strategies/rst_super.c` and `src/strategies/do_organic.c` — self-reporting strategies

Strategies fire treatments without re-entering the pipeline functions, so
they report their own treatments inline. The RST-based strategy journals that
it dispatched its treatment with the sequence number it used. The junk-data
strategy makes a further distinction: if the connection already showed a
response, its action is a retransmission on a live connection, which is worth
noticing in the summary; the check reads the evidence bits first, then marks
the injection evidence. Both build the tuple directly from the raw packet
fields, in the same key convention as the pipeline.

### `src/helper.c` / `src/helper.h` — a read-only consumer

`show_ledger_of` prints a connection's ledger status for debugging, beside
the existing `show_packet`. It looks the record up and reports through the
module's getters, and its declaration joins the other helper prototypes.

### Incidental hygiene

The lines touched in `main.c`, `cache.c` and `helper.c` were re-saved without
their old trailing whitespace at the edit sites; no other formatting or
renaming was done.
