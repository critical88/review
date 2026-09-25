# Injection design record — dormant premium staging in the alpha_vantage client

## Maintenance motivation

The alpha_vantage client sits in the awkward aftermath of a provider program that never
launched. While Alpha Vantage's premium/billable data tier was in beta, the natural place to
harden a Python client is *inside* the client: validate that we are only asking for entitlements
the account can actually pay for, and stop dragging multi-megabyte history payloads through
formatting when a bounded window would serve a preview UI. Two facilities were staged for that
program — a pre-call entitlement preflight and a response preview regime — each installed as a
reversible stage behind a client-side feature switch, and each shipped with the switch turned
down pending the billing contract. The contract was never finalized and the beta was shelved
indefinitely.

This is the shape such staging routinely takes when the enable decision is deferred: the code is
"finished" rather than deleted, parked behind `False` so it can be flipped on the day the program
signs. Everyone intends to revisit it; the program stalls; the staging silently becomes a
permanent resident that every request and every response walks past, that every reader has to
step over, and whose supporting constants, registries, constructor state, helpers, and imports
keep getting reviewed and carried as if they were load-bearing. The change models the residue
that a real maintainer would eventually ask to have retired: coordinated, cross-cutting, dead
under current configuration, and spread over the request path, the response path, the async
variants, and several endpoint families.

## Modeled development evolution

The residue is written as growth, not as one edit. A plausible history for this cluster:

1. The base client grew the request-side gate first, because that is the single place
   (`_call_wrapper`) where endpoint contracts have already returned their function key and all
   call kwargs are known — a natural choke point for validating premium entitlements.
2. The response-side gate followed when the preview program was added, capping series payloads
   in `_format_wrapper` before any output conversion.
3. The shared switches, supported-entitlement set, window limit, constructor-seeded state, and
   the two private helpers were the infrastructure both stages needed, so they live next to the
   wrappers in the base class.
4. When the async client was written (or when the staging was mirrored into it), the same two
   branches were carried into its coroutine wrappers so sync and async behavior would stay in
   lockstep — the repo's async class already duplicates the sync wrapping layers rather than
   inheriting them.
5. As specific payload families were profiled, family-specific tuning accreted where the
   profiling happened: the intraday series (the only endpoints taking a billable `entitlement`
   parameter) grew a pre-gate; the MACD indicator grew a widened window lookup; the FX daily
   endpoint grew a payload downgrade; the digital-currency dailies grew their own window
   mapping. Each family's author picked the locally obvious shape — a helper near the family, a
   per-class table, a value stashed on `self` — so the tuning does not share one uniform
   implementation.

## Overall design

The staging is deliberately anchored to the two cross-cutting wrapper layers in the base class
(`_call_wrapper` for request assembly, `_format_wrapper` for response formatting), from which it
can reach both facilities without touching any endpoint signature. Family-specific tuning then
fans out to four endpoint families. The supporting cast (switches, class constants, constructor
seeds, private helpers, a `collections.deque` import used to bound windows) is distributed the
way long-lived infrastructure actually is: constants and helpers near the base class, tuning
tables near the families they tune, one module-level helper in the family module itself.

Naming follows the library's existing conventions (`_PREVIEW_*`, `_SUPPORTED_*`,
`_verify_entitlement`, `_sample_preview_payload`, `_preview_window_for`) so the added lines read
as house style, and docstrings/comments carry the in-world justification (billing contract on
hold, switch stays down, support reconstruction of rejected requests). The result spans six
production files, 132 inserted lines, and touches request assembly, response formatting,
inheritance, and per-family endpoint contracts — but no public signature, url-formation rule, or
output-format branch changes at all.

## Per-cluster explanations

### Base client — `alpha_vantage/alphavantage.py`

- **Class-side switches and constants** (`_ENTITLEMENT_PREFLIGHT_ENABLED`,
  `_PREVIEW_SAMPLING_ENABLED`, `_SUPPORTED_ENTITLEMENTS`, `_PREVIEW_SAMPLE_LIMIT`, added below
  `_RAPIDAPI_URL`): the two program switches plus the data each stage needs. Class scope is the
  right production shape for toggles meant to be flipped per deployment, and grouping them under
  one comment tells the in-world story of why both sit off.
- **Constructor seeds** (`self._preview_sample_size`, `self._preflight_failures` at the tail of
  `__init__`): instance state the stages need to be call-scoped — a per-instance window the
  families retune, and the reject log refunded to support once a request is refused. Seeding
  them at construction is what keeps the stages stateless across calls; once the stages are
  parked these become write-only bookkeeping.
- **Request-side gate in `_call_wrapper`**: after the endpoint tuple is produced and before the
  url is assembled, the wrapper pre-gates any call that passed an `entitlement` kwarg. This is
  the only place in the sync client where all kwargs and the resolved function name coexist, so
  it is the only spot a preflight could live without changing endpoint signatures.
- **Response-side gate in `_format_wrapper`**: after `func` returns and before the
  json/pandas/csv conversion branches, the wrapper optionally trims the series block. Sitting
  before the conversion branches lets the preview serve every output format at once.
- **Private helpers `_verify_entitlement` and `_sample_preview_payload`** (after `set_proxy`):
  the preflight's supported-set check + reject log, and the window trim. They are instance
  methods because both need the constructor-seeded state; as plain private helpers with the
  library's docstring style they read exactly like the base class's other internal machinery.
- **`from collections import deque`**: the trim helper bounds the retained window with a
  `maxlen` deque — a devoted import that exists to service the sampler alone.

### Async mirror — `alpha_vantage/async_support/alphavantage.py`

The async class re-declares the sync class's two wrapper layers for the coroutine path (it
inherits from the base but duplicates the decorators), so a staging layer aiming for sync/async
parity has to be carried in twice. Both branches sit at the same points (request wrapper after
the function tuple, format wrapper before the conversion branches), with comments noting the
mirror. The helpers, switches, and constants are *not* duplicated — the async class inherits
them, which is realistic inheritance use and also what makes the residue genuinely cross-file:
retiring the shared pieces has to account for the subtype that consumes them.

### Intraday series — `alpha_vantage/timeseries.py`

`get_intraday` (and its `entitlement` parameter, which is a genuine upstream signature used for
realtime/delayed premium data) carries a family-local pre-gate instead of relying only on the
base-class gate: the intraday family is where billable variants actually surface first, and
giving the family its own guarded call is what a developer profiling *this* endpoint would
write. The gate consults the base helper and raises before the API call is spent, so the family
gate is redundant with the wrapper gate by design — mirroring how layered defenses accrete in
real clients.

### MACD indicator — `alpha_vantage/techindicators.py`

`_PREVIEW_SERIES_WINDOWS` is a class-side registry of *callables* (`lambda limit: 3 * (limit or
av._PREVIEW_SAMPLE_LIMIT)`): MACD/MACDEXT emit several output bands per point, so the preview
window for those families is widened by a formula rather than a flat number. `_preview_series_window`
is the resolver that applies the override or falls back to the requested/instance window, and
the gate inside `get_macd` reassigns `self._preview_sample_size` per call before the sampler
would read it. This is the most classically "tuning-table" shape in the change: a registry of
strategies, a resolver method, and a guarded mutation of instance state.

### Foreign exchange — `alpha_vantage/foreignexchange.py`

`_fx_preview_throttle` is a module-level helper (the only module-level function in the change)
that downgrades a `'full'` pull to `'compact'` so the sampler only ever sees light payloads.
The gate in `get_currency_exchange_daily` rewrites the local `outputsize` before returning the
endpoint tuple. The producer-oriented shape differs from every other family on purpose: here
the trimming decision is made on the request kwargs themselves rather than on the response.

### Digital currencies — `alpha_vantage/cryptocurrencies.py`

`_PREVIEW_DAILY_WINDOW_KEYS` is a per-class mapping of series keys to retained row counts, and
`_preview_window_for` resolves the mapping with a fallback to the instance-wide window. The
gate in `get_digital_currency_daily` stores the resolved window on `self._preview_keep` for
the response side — a value that, while the switch is down, is written and never read,
i.e. exactly the write-only state such staging leaves behind when the consumer it feeds is
dark. This family deliberately reuses the *instance-attribute store* shape rather than the
reassignment used in TechIndicators.

## Deliberate structural variation

No single editing pattern clears the whole residue, and no two families implement their hook
the same way. The change intentionally mixes: class-level boolean switches; guarded branches in
two sync wrappers and two async wrappers; a family-local gate that duplicates a base-class
gate; a registry of callables plus a resolver method; a per-class plain mapping plus a resolver;
a module-level plain function; constructor-seeded state; one per-call instance reassignment;
one write-only instance store; and one devoted import used only by a staged helper. The
supporting members also sit at three different scopes (module, class, instance), and the async
class consumes shared members through inheritance rather than duplication. The variety is what
an accreted, multi-authored feature area looks like after several people each tuned the site in
front of them — and it means the useful cleanup is a coordinated removal across scopes and
files rather than a mechanical find-and-delete of one pattern.

## Candidate sites deliberately left out

Repetition would not have made the case better, only longer. The `entitlement=None` parameter
already threads through most indicator and series endpoints (accepted by the provider, passed
in query strings), and only the intraday family gained the billable-variant gate; seeding an
identical gate into `get_sma`, `get_daily`, the fundamental-data and economic-indicator
families, alpha-intelligence, commodities, or the options endpoints would add twelfth-plus
copies of the same guarded call without adding a distinct structural shape. The
`_output_format_sector`/sectorperformance area, the digital-currency-list
`_ALPHA_VANTAGE_DIGITAL_CURRENCY_LIST` url constant, and the pandas-availability
`_PANDAS_FOUND` flag pre-date this change and are untouched — they belong to the client's own
history, not to the staged program. `alpha_vantage/sectorperformance.py` is regenerated by the
repository's install step and is excluded from version control, so it is not a surface this
work touches.
