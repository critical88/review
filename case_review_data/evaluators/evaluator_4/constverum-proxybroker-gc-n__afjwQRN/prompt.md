# This broker does everything now

We embed ProxyBroker in a scraping product, and lately every fix we need has
been landing in the same giant class: the top-level broker that fronts the
package. Tracing how it got here, in the order we hit it:

- We needed the judge set verified fresh on every search. That logic now sits
  in the broker, along with a built-in list of default judges, the pool of
  which judges are currently usable per scheme, the per-scheme events that
  protocol checks wait on, and the random choice of a judge per protocol.
- We needed DNSBL screening, strict anonymity matching, and SMTP relay
  verification. All of that also ended up in the broker: the connect attempts
  and retries, building the raw test request that probes the proxy, decoding
  the (sometimes compressed, sometimes chunked) response, checking that the
  page echoes the expected headers and an IP, and grading the proxy as
  Transparent, Anonymous, or High.
- The two classes that used to run the judge side and the checking side of the
  package are still around and still constructible, but they are empty
  shells now: create one and it merely remembers its arguments. The behavior
  their names promise is executed by the broker, which reads those stored
  fields directly wherever it needs them.

The class is just under a thousand lines and interleaves provider-gathering
coordination, queue bookkeeping, and stats printing with low-level response
parsing and anonymity heuristics. Judge state is process-wide on the broker,
while checking parameters are per-run. We want this cleaned up before the
next feature lands on top of it.

## What we want

Untangle the package along the lines its design implies: give judge
management (defaults, verification, which judges are up, selection) a real
owner again, give the proxy checking pipeline (DNSBL, protocol negotiation,
request building, response parsing, validation, anonymity grading, type
filtering) a real owner again, and let the broker keep its actual coordination
job — gathering, queueing, limits, serving, stats, and lifecycle. Where a
responsibility is really just stored run configuration, keep it as data that
its rightful owner consumes; don't leave the broker reaching into other
objects' fields to interpret a plan.

Hunt down all the logic that crept into the broker — including the small
low-level helpers that came along for the ride — and give each piece a
proper home. The package is small enough that you can audit every module it
ships; make sure nothing absorbed stays behind on the broker.

## Boundaries that must hold

- Nothing public changes: imports, constructor signatures, and the documented
  usage patterns (including the usage examples and the CLI entry point) work
  exactly as before.
- Behavior is identical: for the same inputs, the same scheduling, the same
  queue results with the same terminating sentinel, the same retries, proxy
  log entries, warnings, stats output, and the same judge availability
  semantics (shared within one program run, refreshed for every search).
- The repository's test suite keeps passing unchanged when run in full.
