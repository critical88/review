# Retire the previous-generation helpers left behind by the 4.32/4.33 mechanism swaps

## Observation

The 4.32 and 4.33 releases replaced several long-standing mechanisms:

* mail notifications moved from gomail to the `wneessen/go-mail` driver (#1732, #1733, #1734);
* the shutdown lifecycle was simplified around context propagation (#1704);
* database manifest entries were reworked for consistent handling, with the
  versioned migration chain as the sanctioned way to touch storage layout (#1715);
* config defaults were modernized and obsolete util helpers removed (#1705);
* generated artifact tags and non-image artifacts began being skipped during
  registry checks (#1745, #1746).

Each of those waves rewired the entry points and call sites of the affected
mechanism. The follow-up cleanup that should have accompanied them never fully
happened, and the tree still carries families of helpers from the replaced
generation. They are hard to spot precisely because they look alive: they
reference each other, several are exported, some accept real application
objects (the scheduler client, a signal channel, the database handle), and
their idiom matches the neighborhood they live in. No path the running program
actually takes reaches them anymore.

A recent example of what to look for: the run loop only aggregates analyzed
entries to count statuses for its closing log line, yet a complete run-summary
mail pipeline still exists — a run-summary data type with its own fixed-format
renderer, a digest title template with its parsing helper and its rendering
context, and, in the mail notifier, the mail-subject and body assembly that a
run-end mail would have used. Per-entry templated notifications replaced all of
that; nothing collects a run summary or delivers one today. The same
left-behind pattern exists around the other swaps.

## What to do

Audit the responsibility areas those waves touched, decide member by member
whether a declaration still sits on a live path from the program's real entries
(the CLI wiring in `cmd`, the scheduled and startup runs in the application
package, notifier construction and dispatch, the gRPC service registration,
imported use of the public packages), and delete the previous-generation
families you find, in full:

* the run lifecycle and termination area of the application package
  (`internal/app`) — the pre-simplification, signal-based stop variant and
  whatever exists only to support it;
* the end-of-run digest mail stratum of the pre-template era, which spans the
  notification contracts in the model package, the message-templating
  machinery, and the mail notifier (`internal/model`, `internal/msg`,
  `internal/notif/mail`);
* the storage migration surface of the pre-tag-key layout, next to the
  versioned migration chain it predates (`internal/db`);
* the tag handling of the public registry client (`pkg/registry`) — the
  suffix-filter family of the era before artifact tags were skipped by other
  means.

When judging a declaration, trace its references rather than trusting names,
exportedness or the presence of internal call chains; a family whose only
callers are its own members is residue. Remove whole families, not just their
entry points: their private helpers, the data shapes and template constants
that serve only them, and any package-level seed that exists only for them
must go with them. Where a removal orphans an import or other vestige in the
hosting file, clean that up in the same change.

## What must remain stable

Behavior is unchanged by this work:

* a scheduled and startup watch run still walks every configured provider,
  analyzes images in the worker pool, and logs the closing per-status counters;
* each analyzed entry still dispatches a notification to every configured
  notifier, and the mail notifier still renders the per-entry message from
  the configured template title/body and delivers it through go-mail, including
  its secret retrieval;
* startup still opens the BoltDB database and applies the existing versioned
  migrations; manifest entries stay keyed by the current `name:tag` convention
  with the current helper semantics;
* the registry client still lists tags exactly as the registry returns them
  and sorts them with the configured sort order;
* shutdown still relies on context cancellation propagated from `cmd`.

Compatibility boundary: the public API of `pkg/` (including `pkg/registry`'s
exported surface) and of the generated `pb/` code must not lose or change any
pre-existing symbol. Internal packages keep their exported surface except
where a removal of audited residue requires dropping part of it. No new
dependencies, no rewrites of the replacement mechanisms, no relocation of
live code beyond what the removals justify. The full unit test suite must
continue to pass without modification.

## Working notes

* The build and tests run against the vendored module tree:
  `CGO_ENABLED=0 go build ./...` and `CGO_ENABLED=0 go test -count=1 ./...`.
* Prefer the smallest change that removes the residue completely; this is a
  deletion pass, not a reorganization or a feature port. Do not add call sites
  that would make a retired helper reachable again — retirement is the goal.
