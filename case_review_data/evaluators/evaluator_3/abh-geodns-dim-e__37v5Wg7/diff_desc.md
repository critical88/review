# Zone reload and DNS request handling: inline rework — design record

## Maintenance motivation

GeoDNS lives on two code paths. The administrative path is the zone
reload cycle in the `MuxManager`: every two seconds it watches the zone
directory, notices changed JSON zone files, parses them, and rebuilds
the in-memory zone store that everything else queries. The serving path
is `Server.serve`: for every incoming DNS packet it crunches the question
name, works out the client's geographic targets, walks the zone labels,
and picks which records to answer with.

During the address-format migration work (the sweep that moved the code
base from `net.IP` to `netip.Addr`, and earlier the DNS library v2
migration) these two paths had to be touched over and over, and each pass
meant descending a small ladder of helpers before reaching the code that
actually needed editing. The realistic maintenance story this change
models is a developer who concludes that the ladders are in the way:
the helpers are single-caller indirections on hot or
high-friction paths, every migration needs to unwrap them anyway, and
function-call overhead on the per-packet path is not free. So the helper
bodies get folded directly into the two orchestrators — the zone-loading
work vanishes into the reload loop's per-file block, and the
answer-assembly work vanishes into the request handler — while helpers
that still have other callers are left where they are.

This is the shape such a "shortcut" actually takes in production code:
the orchestrators grow a full parsing engine and a full answer engine
inline, the comments rationalize the folding ("this is the hot path",
"the extra indirection doesn't pay for itself"), and the still-living
specialists keep existing next to fresh in-adapted copies of themselves.

## Normal evolution being modeled

The original architecture is a conventional decomposition:

- `zones/reader.go` owned the JSON zone-file reading:
  `ReadZoneFile` decoded the file into the intermediate data map,
  `setupZoneData` walked the labels and records, `AddLabel`/`addSOA`
  placed labels and the SOA into the zone, and `getStringWeight`
  parsed the optional weights on each record.
- `targeting` owned target-list computation: `GetTargets` assembled
  IP-prefix and global targets and delegated the geographic part to
  `getGeoTargets` (ASN, country, continent, region), with
  `ParseTargets` parsing the zone's target option strings.
- `zones.Zone.FindLabels` walked the targeting chain for a query,
  including the `MF` alias recursion; `zones.Zone.Picker` picked the
  answer records, delegating health filtering to `filterHealth`.
- `server.serve` orchestrated the request: question-name trimming
  (via a small local helper), OPT-record lookup (via the `edns`
  helper), then the targeting, label and picking calls above.

The change collapses those ladders into their two entry points in one
sweep, the way a performance- or migration-motivated patch series does.

## Overall design

Two entry points, in different lifecycle phases of the same service,
each absorb a full chain of delegated responsibilities:

1. `(*MuxManager).reload` (`zones/muxmanager.go`) absorbs the entire
   zone-construction chain: file reading, JSON decoding with the
   parse-error highlighting, zone-option handling, label and record
   construction for every supported record type, weight parsing,
   sub-label wiring for aliases, and SOA assembly.
2. `(*Server).serve` (`server/serve.go`) absorbs the entire
   answer-assembly chain: question-name crunching, the OPT scan, the
   target-list computation, the label walk with alias recursion, and
   the record picking with health filtering and proximity weighting.

The absorbed copies are not verbatim pastes: locals are renamed to read
naturally in their new home, early returns become `break`/flag patterns
where the surrounding function cannot return, struct literals are
keyed, and package-qualified references drop their qualification where
the host already imports the package. Where absorbing a helper outright
was impossible or pointless because other callers exist, the helper
survives and the orchestrator keeps calling it for those other paths
(`AddLabel` for the SOA labels, `Zone.Picker` for the ANY case,
`GetTargets` for the real-IP fallback and the `_country` debug answer).

## Per-location rationale

### `zones/muxmanager.go` — `(*MuxManager).reload`

What changed: the reload routine, previously a compact orchestration
around a call to the reader package, now performs the zone-file work
itself, per changed file, inside an immediately-invoked `zoneErr`
closure that preserves the deferred `recover` semantics the file parser
used to provide (a panicking record constructor must still be logged
and must not kill the reload cycle).

Site and form were chosen because:

- The reload loop already owned the per-file lifecycle — hash check,
  read, publish or drop, register the handler. Making it also own the
  parse means the whole file-to-zone transformation is readable in one
  place, which is exactly the argument such reworks are sold on.
- The closure-per-file form keeps the `defer`/`recover` contract
  intact: defers run when the closure returns, so the panic-isolation
  behavior of the old parser entry point survives without keeping the
  entry point itself.
- Inside the closure the phases follow the old chain's order — open and
  stat the file (the zone serial still derives from the file's
  modification time), decode the JSON with the byte-offset error
  highlighting intact, apply the zone options, then build labels and
  records, then geo-register the zone — so the production role of each
  phase is unchanged even though the functions that used to hold the
  phases are gone.

Specific absorbed blocks:

- *Targeting-option assembly.* The zone's `targeting` option string is
  split and folded into the target bit-flags inline (the eight-way
  option case, including the shared error for an unknown option). This
  string-to-flags logic previously lived in the targeting package's
  parser, which survives for its other callers.
- *Label and record construction.* The per-label and per-record setup
  walk now runs inline: labels are created lowercased into the zone's
  label map, and each record list is spread across the full record-type
  switch (A, AAAA, PTR, MX, SRV, CNAME, MF, NS, TXT, SPF) with weight
  parsing performed on the spot (the `["value", 10]`-shaped two-element
  entries), `TXT`/`SPF` string joining, sub-label wiring for the
  `MF`/alias convention, and the closing pass that applies the zone's
  TTL and weight defaults to labels created along the way.
- *SOA assembly.* The zone's SOA record is assembled inline from the
  zone's parameters, with the fallback path that stamps a SOA label on a
  fresh empty label still delegating to `AddLabel` (kept for the
  `setupPgeodnsZone` path, which also constructs SOA-bearing zones and
  which is not on this rework's path).

The surrounding bookkeeping is untouched: the sha256-based change
detection with its comment about the read-twice race window, the
removal of deleted zone files, the `lastRead` hash map, the handler
registration, and the skip-and-continue on a bad file (other zones keep
serving — an operational guarantee that must not regress).

### `zones/reader.go`

What changed: the file that used to hold the whole reader chain
(`ReadZoneFile`, `setupZoneData`, `getStringWeight`) shrank to just the
`ZoneList` type declaration, because every one of those functions lost
its only caller to the reload closure — the parse/construct bodies now
live in the reload routine, and the weight parsing exists only as the
inline handling in the record switch.

Why stop at the type and not delete the file: `ZoneList` is the typed
map used by the server startup and several call sites; relocating it
would mean touching imports across files for no design gain. Shrinking
a specialist file to its surviving bits is what this kind of rework
naturally leaves behind.

### `server/serve.go` — `(*Server).serve`

What changed: the per-packet answer assembly, previously a sequence of
delegations, is performed directly in the handler:

- *Question-name crunching.* Trimming the zone-origin suffix,
  splitting on dots, dropping the zone's label count, and lowercasing
  the result moved out of the local one-caller helper (deleted) into
  the top of the handler, next to the query-log bookkeeping that
  already lived there.
- *OPT scan.* The scan for the request's OPT pseudo-record (previously
  one call into the `edns` package) became a plain range-and-type-assert
  loop in place. The `edns` helper survives: version negotiation and
  the size-and-do handling still call it. The included EDNS-client-subnet
  handling is untouched (address validity, global-unicast vs
  private/link-local filtering, query-log fields, the real-IP
  fallback).
- *Target-list computation.* The handler assembles the target list
  itself: the IP-prefix targets (`[ip]` plus the /24 or /48 masked
  address), the ASN target, the country/continent/region/region-group
  targets behind their option flags, and the global `@` suffix. The
  two-tier structure (prefix targets, then the geographic part),
  provider-lookup failure behavior (location lookup failed means only
  the ASN target can be offered, location stays nil), and the netmask
  return for EDNS scope are all reproduced locally against the same
  geo provider interface. The library's `GetTargets` remains in use
  for the retry against the real IP and for the `_country` debug
  answer, so both copies stay live in production.
- *Label walk.* The FindLabels algorithm — resolve each target to a
  label name, probe the wanted qtype with the MF/CNAME prelude, follow
  `MF` alias chains via the trimmed type list (still assembled with the
  `slices` helper), and fall back to the empty label match so unknown
  names answer NOERROR-with-SOA rather than NXDOMAIN — now runs inline,
  appending to the match list the handler then iterates. The alias
  step still calls the original method for the recursive legs.
- *Record picking.* The picker — health filtering of records under
  test, the unweighted pass-through for groups with no weights, the
  proximity pass choosing records within 5% of the minimum distance,
  and the weighted random draw bounded by `max_hosts` (CNAME and MF
  capped at one) — became a single one-shot loop whose `break`
  statements stand in for the old function's early returns. The
  ANY-qtype case still routes through `Zone.Picker`, whose other users
  remain clients of the original.

Why `serve` as the second host: it is the literal hot path, so it is
where the "zero indirection" argument is most credible and most
damaging. Every absorbed block is reachable only through a request, so
the whole engine is re-derived per read of the function, which is the
actual maintenance cost of this design.

Everything around the absorbed blocks is preserved: the query-log
deferred entry with its answer post-processing, the metrics (with the
zone/qtype/qname/rcode labels), the `_status`/`_health`/`_country`
debug labels, EDNS scope propagation, and the SERVFAIL fallback when
packing the response fails.

## Spread and variation

The rework spans four production areas: the reload lifecycle, the zone
store's construction chain, the geo targeting module, and the per-packet
serving path. The two absorbing entry points sit in different lifecycle
phases (administrative reload vs. per-packet serving) and pull the same
kinds of construction responsibilities into themselves in two
structurally different ways — one inside a per-file closure that mirrors
the old parse function's error and panic contract, the other as a flat
top-to-bottom request script with control-flow scaffolding replacing
helper boundaries. The inline copies were adapted, not copied: locals
renamed (`sha256sum`, `targets`, `labelRR`), early returns restructured
(the `geoOK` flag, the one-shot picking loop, the `location = nil`
reset on the failed lookup), literals keyed to the new surrounding
style. The surviving specialists (`AddLabel`, `addSOA` via `AddSOA`,
`ParseTargets`, `GetTargets`/`getGeoTargets`, `FindLabels`, `Picker`,
`filterHealth`, the `edns` OPT helper) keep their original contracts
and their other callers.
