# Injection design record — uniform record model across reform's core row and view contracts

Repository: [AlekSi/reform](https://github.com/AlekSi/reform), module `gopkg.in/reform.v1`, pinned commit `db2c976d4b2fa1df1f9e16b4c314e18c5b475969`. Diff shape: 7 files, +499/−133 lines, 44 hunks.

## 1. Maintenance motivation

reform describes every SQL relation with two artifacts: a descriptor object
implementing `reform.View` (schema, table/view name, column list, row
factory) and row types implementing `reform.Struct` (string/print form, values
and pointers for scanning). Relations that are tables with a single-column
primary key get a wider, more convenient contract:
`reform.Table` extends `View` with `NewRecord()` and `PKColumnIndex()`, and
`reform.Record` extends `Struct` with `Table()`, `PKValue()`, `PKPointer()`,
`HasPK()` and `SetPK()`.

That split has long been a friction point in the library's own issue tracker
style of feedback: newcomers expect a row object to answer *all* questions
about itself, and the library's internal query code must repeatedly re-discover
which rows are records with runtime assertions at nearly every step of the
INSERT/UPDATE/DELETE pipeline. During a hypothetical v1.6.0 cleanup cycle a
maintainer proposes to collapse that distinction into a "uniform record
model":

- one constructor, one accessor surface, one lifecycle predicate for every
  row, regardless of relation kind;
- `Table` and `Record` survive only as deprecated type aliases, so every
  downstream import (models generated in user projects, `reform-db`, addons
  such as gorp compat or the pgx driver setup) keeps compiling;
- the query pipeline stops doing per-call type assertions and reads the
  record capabilities straight off the row interface.

The tour through the library's docs and release notes makes this exact kind of
"make the record contract uniform, deprecate the old names" change a credible
piece of development: it is API-surface driven, documentation driven, and
motivated by simplifying both user-facing docs (`Table`, `Record`, views) and
internal query construction.

## 2. Evolving development change being modeled

The injected change is modeled as one developer's branch preparing the
v1.6.0 release notes. In a normative evolution this lands as a sequence of
commits: (1) widen the core interfaces and alias the record-specific names
with a deprecation note pointing at v1.6; (2) teach the generator template to
emit the widened surface for every relation, with view-shaped default bodies;
(3) regenerate the model files kept in-tree (`internal/test/models`, and the
`reform-db` metadata models) so the repository stays consistent with its own
generator, preserving the `parse.AssertUpToDate` guarantees; (4) drop the
runtime narrowing in `querier_commands.go` in favor of the uniform members;
(5) write the changelog entry. All experiments below treat the resulting
state as a single staged patch rather than an artifact of edit history.

## 3. Overall design

The design gives the two core contracts the full "record and table" surface
while keeping the exported identifier set unchanged:

- `base.go`: `View` gains `NewRecord() Record` ("or nil for views and tables
  without primary key") and `PKColumnIndex() uint` ("or zero ..."); `Struct`
  gains `Table() Table`, `PKValue()`, `PKPointer()`, `HasPK()`, `SetPK()`
  with matching view-tolerant wording. `Table = View` and `Record = Struct`
  become documented deprecated aliases.
- View-descriptor owners implement the two new `View` members with inert
  defaults: `NewRecord` returns nil for a view, `PKColumnIndex` returns 0.
- View-row owners implement the five new `Struct` members with inert
  defaults: `Table`, `PKValue` and `PKPointer` return nil, `HasPK` returns
  false, `SetPK` is an empty body.
- `querier_commands.go` reads the record capabilities through `str.Table()`
  and the uniform members instead of asserting to `Record`/`Table`.
- `reform/template.go` renders the full ten-member surface for every
  generated model, with `{{ if .IsTable }}` branches selecting the real
  primary-key bodies or the inert view bodies.
- The generated model files in-tree are regenerated from the new template:
  the fixture models under `internal/test/models` (used by the entire test
  suite) and the production metadata models of `reform-db` — the models that
  `reform-db init` produces for reading `information_schema` (`tables`,
  `columns`, `key_column_usage`) and sqlite's `sqlite_master` /
  `sqlite_table_info`.
- The `CHANGELOG.md` v1.6.0 section documents the uniform record model and
  the regeneration instruction, the normal vehicle the project uses for
  API-visible work.

Behavioral goal during construction: every supported workflow — selecting
rows of any relation, inserting/updating/deleting *records* of tables, saving,
bulk-inserting PK-consistent batches, reading view rows in `reform-db` — keeps
producing identical SQL and identical results. Views stay read-model rows;
nothing new calls the inert members on them.

## 4. Per-location design

### 4.1 `base.go` — the contract definitions (+29/−20)

The two core interfaces are the only place where the surface of the uniform
model is defined, so they come first. Extending `View` and `Struct` in place
(rather than introducing a new parallel interface pair) is what a maintainer
blaming "two contract tiers" for user confusion would do: the exported names
users already know keep their meaning, and the doc comments absorb the new
"or nil for views" semantics that the rest of the change relies on.
`type Table = View` / `type Record = Struct` aliases with `Deprecated: do not
use, it will be removed in v1.6` comments preserve the exported API set, the
same pattern `database/sql` popularized for transitional aliases. The
`SetPK` deprecation note on `Record` moves onto the shared `Struct` member,
which is the wording change the template mirrors for generated files.

### 4.2 `querier_commands.go` — query pipeline consumers (+34/−26)

`filteredColumnsAndValues`, the private `insert`, `Insert` and `InsertMulti`
are exactly the functions that previously performed
`record, _ := str.(Record)` / `view.(Table).PKColumnIndex()` narrowing — one
private helper and three public pipeline entry points. The uniform model
replaces each assertion with a local `table := str.Table()` (nil for rows of
relations without a primary key, real for records of tables) and the
`table != nil` guards that follow it. This is the same before/after shape in
all four sites because that is where the pipeline consults record-specific
capabilities: rejecting PK-column updates in `filteredColumnsAndValues`,
choosing `OUTPUT INSERTED`/`RETURNING` and scanning the assigned key in
`insert`, cutting the PK column before insert in `Insert`, and enforcing
all-or-none PK presence in `InsertMulti`. Their shared private role is why
they change together and why the change reads as one mechanical conversion of
the narrowing idiom. Public `Update`, `UpdateColumns`, `Save` and `Delete`
already operate on records, so they require no edit: with the alias, their
`Record` parameters mean `Struct` and the compile-time acceptance widens with
no wording change in behavior — an accepted consequence of the uniform model,
since record-requiring operations on view rows have never been part of
supported usage.

### 4.3 `reform/template.go` — the code generator (+46/−24)

Every model file in every user project is rendered from `structTemplate`,
so the generator is where the uniform surface becomes the *default* for the
ecosystem rather than a property of the in-tree files: emitting
`NewRecord`/`PKColumnIndex` on `{{ .TableType }}` and the five record
members on every `{{ .Type }}`, with `{{ if .IsTable }}` selecting real
primary-key bodies or inert view bodies, plus an unconditional conformance
block (`_ reform.View`, `_ reform.Struct`, `_ reform.Table`,
`_ reform.Record`, `_ fmt.Stringer`). Keeping the branches keyed on
`.IsTable` matches how the template already distinguishes view/table
relations. (One escaping subtlety: the generated SetPK deprecation comment
references the primary key field, which views do not have, so that comment
line itself moves inside the table branch.)

### 4.4 Regenerated model files

- `internal/test/models/good_reform.go` (+149/−45) and
  `internal/test/models/extra_reform.go` (+26/−18): the fixture models that
  every query test compiles against. Regenerating them from the extended
  template propagates the uniform surface to `PersonProject` and
  `CompositePk` (the in-repo examples of keyless and composite-key view
  rows), keeps `parse.AssertUpToDate` honest, and keeps the checked-in
  `.ReformView`/`.ReformTable` markers exactly what the tool would
  produce. This is the standard way this project consumes its own generator:
  for this repository the generator is also a model consumer, and the test
  suite compiles against these files.
- `reform-db/models_reform.go` (+210/−0): the production models that
  `reform-db` init/query/exec emit and read against — `information_schema`
  relations (`tables`, `columns`, `key_column_usage`) and the sqlite
  introspection relations (`sqlite_master`, `sqlite_table_info`). These
  are the widest body of authentic *view* relations in the repository and
  therefore the production-facing prove-out of the uniform model: five view
  descriptors and five view rows, each gaining the inert members. The file
  is pure additions because those relations were all keyless views before.

### 4.5 `CHANGELOG.md` (+5/−0)

The v1.6.0 (not released yet) section records the arrival of the uniform
record model, the alias status of `reform.Table`/`reform.Record`, and the
instruction to regenerate generated models — the project's normal way to
tell downstream users about an API-visible change, and the anchor a reader
would use to understand why the contracts widened.

## 5. Why the change stops where it stops

The uniform model deliberately stops at the core row/view contracts and the
paths that consume them. The optional hook interfaces (`BeforeInserter`,
`BeforeUpdater`, `AfterFinder`) are queried by runtime assertion and are
already the "optional capability" pattern; collapsing them into `Struct`
would force generated stubs onto every model while the fixture models in-tree
hand-write real hooks (`Person`, `Project`), which the generator — writing
each file before seeing any others — is not in a position to reconcile, so a
realistic maintainer leaves them alone. Wider contracts (`Dialect`, `DBTX`,
`DBTXContext`, `Logger`) describe drivers and connections rather than rows
and have no part in a record/tier unification. `reform-db/models.go` and
the other hand-written model files define no contract members and implement
none, so they are untouched.
