# Injection design record — kiwi-intl 1.3.0 maintenance branch

This record documents the design of the changes in `smell.diff`: what was built,
why the work was motivated, why each location took the shape it took, and what
production role each piece serves. It is a written-for-a-colleague design
rationale for the evolution being modeled. Whether any given hunk participates
in a particular design hazard is deliberately left to review; this record does
not assert a verdict over its own diff.

## Maintenance motivation

kiwi-intl is the runtime internationalization engine behind the kiwi
localization toolchain: `IntlFormat.init` builds an `I18N` instance over the
per-language message tables and hands back a reactive proxy that applications
call at runtime (`get`, `template`, `setLang`, `formatDate`, ...).

Through the 1.2.x line the engine accumulated operational pain that the CLI
team kept working around by hand:

- **No visibility into message usage.** When a product asked "which keys are
  actually hot in the running app" or "which keys keep falling back to the
  default language", the only available answer was to instrument messages from
  the outside, per application, over and over.
- **Money rendering inconsistencies.** Applications formatted prices
  themselves, so each one hard-coded its own currency, grouping and decimal
  conventions; the zh-CN and en-US marketplace pages disagreed.
- **No cache insight.** Rendered-message caching was invisible: nobody could
  tell whether a slot was being reused, how many entries were resident, or what
  a safe prune would remove.

The 1.3.0 maintenance release was scoped to fix exactly this: give the
resolution pipeline a first-class observability surface (per-message records,
per-language datasets, a usage journal, a query API on the facade) and make
number rendering locale-correct by moving market conventions into the engine.

## Development evolution being modeled

The work models the most common shape such a release actually takes in a
living codebase:

1. A maintainer extends the module they already know — the facade class and its
   surrounding `src/` folder — by adding new collaborator types for the new
   bookkeeping. The collaborators start as small, struct-like record types
   (fields plus trivial accessors) because the first milestone is "capture the
   data".
2. The behavior that should consume each new record gets written wherever its
   author happened to be standing when the data became reachable — on the
   facade, on the formatter, on the store — because the deadline was the
   feature, not the archaeology of "which class should own this line of
   logic". Reaching into a neighbor that is already in scope is the path of
   least resistance.
3. Each feature lands with tests against the public surface it exposes, so the
   branch is green and shippable even while knowledge placement lags behind the
   object graph it built.

This is the normal failure mode of organic growth: the object graph grows, the
placement of behavior does not follow the data. The new classes stay thin
because their would-be methods were written by whoever held a reference to
them.

## Overall design

Five new collaborator types are introduced around the untouched facade, plus
the facade integration that wires and exposes them, plus branch-harness
updates and an expanded in-module test suite.

- **`MessageEntry`** — one record per (language, key) pair: content plus
  lifecycle metrics (hit count, hotness level, cache writes, last-use and
  render stamps, render sequence) and cheap predicates.
- **`LanguagePack`** — one dataset per language: the raw message table, its
  default-language reference, usage bookkeeping (hit timestamps, distinct
  misses, a promotion hot-window) and market conventions (currency, grouping,
  digit ranges).
- **`TranslationCache`** — a slot store for rendered output keyed by identity,
  with recall/admission counters and an oldest-slot probe.
- **`IntlFormatter`** — the ICU machinery: message compilation with a
  failure journal, number and date rendering driven by locale and options.
- **`IntlAudit`** — a usage journal: enable flag, record list, begin/line
  mechanics.

The facade (`index.ts`) gains the registries (packs, entries, cache,
formatter, audit), a resolution pipeline that reuses its existing
active-pack/default-pack fallback chain, and the public surface
(`getDetailed`, `describe`, `stats`, `audit`, `formatNumber`, `formatDate`,
`formatCurrency`, `hotKeys`, `pruneTranslations`). The pre-1.3 API
(`init`, `setLang`, `template`, `get`, `getProp`, the Observer proxy) is kept
byte-compatible; all new internal state uses the same double-underscore field
convention as the existing instance fields so the proxy pass-through keeps
working unchanged.

Not every piece of this branch is part of the same story: the version bump and
toolchain pinning are release logistics; the new suite is specification; and
the collaborators exist because the release promised them, not because any
single design decision required all nine files. The per-cluster notes below
record, for each piece, where the behavior consuming the new data was placed
during this evolution and why it landed there.

## Per-cluster explanations

### Cluster 1 — the per-message record type (`src/MessageEntry.ts`)

**What.** A field-and-accessor record: key, language, origin path, content,
and the usage metrics (hit count, hotness level, cache writes, last used,
last rendered language, render count). Predicates only answer questions about
the record's own data.

**Why this shape.** Capturing the per-message data was milestone one of the
release; the team consensus was "capture first, decide behavior later", so the
type deliberately shipped as a data record. Its production role is to be the
single place a message's lifecycle facts live, so that every consumer (cache,
journal, diagnostics, hot-key ranking) reads the same numbers instead of each
keeping a private shadow copy.

**Where the behavior went.** The routines that operate on one record's
lifecycle metrics were written by the consumers holding the reference, not as
record methods: the promotion check that ranks the record's own hotness lives
with the dataset's hot-window bookkeeping (cluster 2); the render step that
pulls the record's content and language and stamps render state back onto the
record lives on the formatter (cluster 4); the slot-store admission and
eviction steps that mint keys and verdicts from the record's metrics live on
the store (cluster 5); the journal line that serializes the record's metrics
lives on the journal (cluster 6); the human-readable self-description is
assembled on the facade (cluster 7). During this evolution the record itself
gained none of those behaviors — it answers questions, it does not narrate,
render, rank, admit, evict or journal itself.

### Cluster 2 — the per-language dataset type (`src/LanguagePack.ts`)

**What.** One owner per language: raw message access with default-language
awareness, hit/miss bookkeeping (recent hit timestamps capped at 16, deduped
distinct misses, a hot-window of promoted keys), and the market conventions
(currency, grouping, digit bounds) needed for money rendering.

**Why this shape.** The dataset owns usage bookkeeping because the release
framed "usage" as a property of the language table being consumed; market
conventions live here because they are per-locale facts (zh-CN renders CNY
grouped, en-US renders USD), and the team wanted exactly one table of such
facts per language.

**Where the behavior went.** The promotion rule — whether one message record
ranks as hot — was placed here as part of the hot-window lifecycle: the author
was implementing the window bookkeeping and wrote the ranking arithmetic for
each candidate record inline, reading the record's hotness, hit and
cache-write metrics and its last-use timestamp, all in one boolean expression.
The two summaries of a dataset (the aggregate statistics report used by
`stats()`, and the market conventions for currency rendering) were likewise
placed with the code that already held a reference: the facade (cluster 7)
reads the dataset's counters, hot-window and conventions field-by-field to
compose its statistics report, and the formatter (cluster 4) pulls the
dataset's currency, grouping and digit facts to build money output. The
supplied `shouldPromote`/`raw`/`note*` surface is what `hotKeys`, the
resolution pipeline and the tests consume.

### Cluster 3 — the unresolved-integration hub on the facade (`src/index.ts`, resolution pipeline)

**What.** Facade fields for the five registries, constructed in the existing
constructor; `__packFor__`/`__activePack__` dataset lookup-or-create;
`__resolveEntry__`, the pipeline that reuses the module's existing fallback
chain to fill a record's content (active dataset's text, else default
dataset's text, else the key itself — mirroring the untouched `get()`), stamp
its hit, note the dataset's hit/miss, and feed the hot-window.

**Why this shape.** Resolution has to live on the facade: only the facade
knows the active language and the default-key policy that the fallback chain
depends on. This cluster is deliberately integration code: it wires
collaborators together through registry-mediated calls and hands the resulting
record to the consumers below.

### Cluster 4 — the rendering owner (`src/IntlFormatter.ts`)

**What.** The ICU machinery, isolated behind one class: message compilation
with a failure journal (an unformattable template logs a warning, journals the
offending text and yields the empty string, exactly like the legacy `get()`
path), locale-driven number and date rendering, and an options-defaulting
merge.

**Why this shape.** All `Intl.*` interaction already concentrated in one place in
the original module; the new class preserves that (one seam for the ICU
dependency, one failure journal reused by every render).

**Where the behavior went.** The 1.3 work extended this class in place,
because it was the rendering class. `renderEntry` grew to take a message
record and drive rendering *from the record*: it pulls the record's content
and language, compares against the record's last-rendered language to detect
drift, calls the shared message compiler, stamps the record with the render
outcome and appends the record's key to the failure journal when the text
would not compile. `packNumber` grew to pull a dataset's currency, grouping
and digit bounds and derive the target locale from it. The option-merging
helper reads each caller-supplied option exactly once — it assembles the
formatter's own working record — and was kept single-read (a first draft read
fields twice for guard-and-assign; that was collapsed during construction
because double reads duplicated the reach for no clarity gained). The
production role of the whole cluster is deterministic, journal-aware ICU
rendering for both plain and money values.

### Cluster 5 — the slot store (`src/TranslationCache.ts`)

**What.** A string-slot map with recall/admission counters, an oldest-slot
probe, plus the two released entry-level operations: admission
(`rememberEntry`) and eviction (`evictEntry`).

**Why this shape.** Slots are the store's own state, so slot insertion and
lifecycle (counters, hot-slot markers, eviction order) belong here. The store
was designed as the single authority on what is resident.

**Where the behavior went.** Both entry-level operations ended up deriving
their answers from the record's metrics rather than the store's own data: the
slot key and hot-slot marker are minted from the record's identity fields and
hotness level, the miss counter resets when the record's stamp says this is
its first constructive cache write, and the record's cache-write counter is
incremented as a side effect of admission; the eviction verdict hinges on the
record's hit count, hotness level, identity and usage stamps, with the store's
own slots consulted only to protect what it actually holds. The author wrote it
this way because the record was in hand and carried every relevant fact the
answer needed. Production role: released `pruneTranslations` and
`getDetailed` go through exactly these two operations.

### Cluster 6 — the usage journal (`src/IntlAudit.ts`)

**What.** Journal storage and mechanics: enable flag, record list, begin
hook, line assembly, list sizing.

**Why this shape.** The journal must be toggle-able and append-only; storage
and lifecycle are its own state and stay on the class.

**Where the behavior went.** Content formatting went to whoever held the
record: the usage-event line was inlined into `recordEntryUsage` by reading the
record's key, language, hit count, hotness, cache writes, last-use timestamp
and render sequence straight into a template string. Instance-level context
went the other direction in `attachInstance`: the journal reads the facade's
language, packs, active keys, default key, entry registry and cache counters
wholesale to assemble an instance snapshot header for the audit sweep. The
production role of this cluster is the shipped `audit()` report: the facade
begins a sweep, snapshots its own state through the journal, then records one
usage line per resident record.

### Cluster 7 — the facade surface (`src/index.ts`, feature API)

**What.** The released API: `getDetailed` (resolve, recall, render, admit,
journal and return `{ value, entry }`), `describe`, `stats`, `audit`,
`formatNumber`, `formatDate`, `formatCurrency`, `hotKeys`,
`pruneTranslations`, plus the two diagnoser helpers (`describeMessage`,
`packStatistics`) that back `describe` and `stats`.

**Why this shape.** Every released feature is a facade method because
consumers only see `IntlFormat` — the facade is the module's public contract,
and making the new features reachable without changing the proxy was the
release's compatibility requirement.

**Where the behavior went.** `describe`'s implementation narrates a message
record field-by-field inside the facade's diagnoser helper — the author was
writing the facade's reporting surface, and the record's metrics were one
reference away. `stats` reads a dataset's counters, hot-window and conventions
the same way for the same reason. The pipeline-style methods
(`getDetailed`, the format passthroughs, `formatCurrency`,
`hotKeys`, `pruneTranslations`) route through the collaborators and mostly
carry integration logic; the two diagnoser helpers are where the facade
stopped delegating and started reading.

### Cluster 8 — release harness (`kiwi-intl/package.json`, `kiwi-intl/tsconfig.json`)

**What.** Version bump to 1.3.0 for the maintenance release; dev-toolchain
pinning (mocha/ts-node/TypeScript versions pinned for the module's suite) and
a ts-node registration block in `tsconfig.json` so the suite compiles under
the commonjs setting the runtime uses.

**Why.** The release branch must reproduce identical suite runs across
machines and CI images; the engine's own dependency on the ICU runtime stack
is unchanged. These files are release logistics and build reproducibility
only; they carry no runtime behavior.

### Cluster 9 — the expanded suite (`kiwi-intl/test/i18n.test.ts`)

**What.** The module's suite grew from the 59 baseline cases to 103: the
original sections run unmodified, and the new sections pin the released
surface — the detailed resolve path, record and dataset bookkeeping,
diagnoser and statistics output, audit journal behavior (enabled and
disabled), market formatting, and cache maintenance.

**Why this shape.** The release specified behavior by tests against the
surface consumers actually call — including the collaborator types directly
where their bookkeeping is part of the released contract (window caps, miss
dedup, slot-store counters, journal mechanics). Tests assert observable values
and shapes; they do not prescribe where the code that produces those values
lives, so knowledge placement remained a matter of authoring convenience
rather than a specified outcome. One deliberate consequence: a follow-up that
reshuffles responsibilities between the collaborators keeps this suite green
as long as the observable surface holds, which the team accepted as the
compatibility boundary of the release.
