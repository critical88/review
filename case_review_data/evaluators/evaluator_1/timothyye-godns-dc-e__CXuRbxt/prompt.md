# Refactor: restore the domain record across the per-domain DNS update flow

## Situation

godns supports per-domain DNS providers: a domain entry in the configuration
may name its own provider, and when it does not, a global provider is used.
A recent refactor detached the update flow from the settings model to "stop
leaning on the whole configuration object":

- The per-domain background update loop, the one-shot update entry, and the
  DNS record-update step now each declare a domain's three values — its name,
  its subdomain list, and its (possibly empty, meaning "global") provider —
  as individually typed parameters.
- The manager's run loop destructures every configured domain entry into
  those same values before handing them to the loop or one-shot path.
- Per-domain provider resolution inside the update flow takes a two-value
  subset of the same group, supported by a single-field fallback helper that
  resolves an empty provider to the global one.
- Startup validation of domain entries (in multi-provider mode) was extracted
  into a helper that validates the re-declared field values plus the config.

Everywhere else in the codebase a domain is still one record: the
configuration model defines the domain entry as a single struct with exactly
these fields (used as a unit by the provider factory and its domain-provider
accessor).

## Problem

(a) Every consumer of a domain in this flow re-declares the same parameter
group, so adding (or renaming) a domain field means hunting every signature
along the update path. (b) The answer to "which provider owns this domain?",
which the configuration record already provides through one accessor, has
drifted into a field-level fallback next to a scattered subset signature.
(c) The domain values have to travel together for the code to be correct at
all — the loop, the update, and the provider lookup can't act on a partial
domain — which is the signature of an eroded record abstraction.

## Task

Reintroduce a single record abstraction for the domain throughout the update
flow, preserving behavior exactly:

- The update flow's methods (loop entry, one-shot entry, record-update step)
  should accept the domain as **one value** — reuse the existing domain
  record type from the settings model, or introduce an equivalent single
  record grouping these fields — instead of re-declaring its name, subdomain
  list, and provider as separate parameters.
- Provider resolution during an update should work from that record, with
  one authoritative place answering "which provider handles this domain"
  (domain-specific provider preferred, global fallback otherwise).
- The manager's per-domain fan-out (background loop and RunOnce path) should
  hand the record to the update flow rather than destructuring entries into
  loose locals first.
- Startup per-domain validation should receive the domain record (plus
  whatever surrounding config context it needs) instead of re-declared field
  values.
- Clean up leftovers of the field-passing form rather than leaving dead
  material behind: the single-field fallback helper and any field-level
  shims that end up unreferenced should be removed or absorbed into the
  record-based logic, not bypassed.
- Adapt call sites that follow the flow's signatures — including in the
  existing tests — as a mechanical consequence of the change.

Investigate the whole per-domain path — the manager's fan-out, the update
engine, its provider resolution, and the startup validation of domain
entries — and address every site where a domain currently travels as loose
values. This is a behavior-preserving refactoring; do not add features, do
not change the configuration file format or flags.

## Compatibility boundary (must hold after your change)

- Per-domain loops still run one update immediately, then on the configured
  interval, and exit promptly when the shared context is cancelled;
  concurrent loops still shut down together (the existing loop-lifecycle
  tests must keep passing, with adapted call sites).
- RunOnce mode still performs exactly one synchronous update per domain and
  still exits non-zero when a per-domain update fails.
- Updates are still skipped while the current IP equals the cached IP, and
  the cache still advances only after a successful provider update.
- Per-domain provider selection semantics are unchanged: a domain-specific
  provider wins; a domain without its own provider uses the global one; the
  success notification still identifies the provider that handled the update.
- Startup validation outcomes are unchanged: empty domain names and
  subdomains rejected; a domain-named provider missing from the providers
  map rejected; domains without their own provider still accepted when a
  global provider is configured.
- The provider backends' three-parameter update contract, the webhook and
  notification APIs, and the configuration/CLI surface are out of scope and
  must not change.
