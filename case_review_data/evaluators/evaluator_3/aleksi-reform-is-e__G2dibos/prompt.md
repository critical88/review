# Refactor request: restore segregated record contracts in the reform core

## Where this comes from

On our v1.6 branch we adopted a "uniform record model" meant to simplify the
library: the general row and relation-descriptor contracts in the core
package were widened to carry the full record surface (constructing new
records, locating the primary key column, exposing an owning-table
descriptor, reading and scanning the key, reporting key presence, and key
assignment), and the historic record/table contract names were reduced to
deprecated aliases of the general ones. Reading the result now, the design
mixed two responsibility tiers into one shared surface:

- Capabilities that describe **primary-key state** only exist for relations
  with a single-column primary key. The repository legitimately models other
  relations as first-class rows — read-only SQL views, relations with
  composite keys, and the `information_schema` / sqlite introspection
  relations that `reform-db` reads. Those owners now have to carry the record
  surface with placeholder bodies that return nil or a constant, and the
  model generator emits the same placeholders for every future view model.
- The insert, bulk-insert and column-filtering internals that once
  distinguished records from plain rows at runtime were rewritten to assume
  the uniform surface on every row.

We want the responsibility boundary restored before the release.

## What we are asking for

Investigate the row/view contract tiering across the codebase and return the
record capabilities to a segregated design:

- Review the core package's row, relation, record and table contracts, and
  separate the primary-key-dependent capabilities from the shared
  schema/row surface that every relation genuinely needs. Owners without a
  single-column primary key should depend only on members that have real
  meaning for them; the placeholder members should disappear from their
  shape, whatever interface design you choose.
- Apply the same design end to end rather than in one silent corner: the
  in-tree generated models (the fixture models our whole test suite builds
  against, and the production metadata models that `reform-db` uses for
  `information_schema` and sqlite introspection reading) and the model
  generator that renders them all have to agree with the contract change —
  regenerating any model with the in-repo generator after your change must
  produce exactly what is checked in.
- Rework the internals that consume record capabilities — the insert paths
  (all dialect conventions for receiving a new key), the bulk-insert path
  with its all-or-none key consistency rule, and the column filtering used
  by inserts and updates — so they obtain those capabilities in a way that
  follows your contract design instead of assuming every row has them.
- Bring the written record with you: the deprecated record/table names must
  keep their documented meaning for a class of rows where it is real, and
  the unreleased section of the project changelog must describe the design
  you actually delivered rather than the one you replaced.

## Behavior and compatibility requirements

- No behavior change for supported workflows: selecting rows of any
  relation; inserting, saving, updating and deleting records of tables;
  bulk-inserting key-consistent batches; the `reform-db` metadata reading
  flows; the error contract for attempts to update key columns.
- Source compatibility for existing downstream projects: model files that
  were generated for tables and records keep compiling against the core
  package, including through the deprecated names.
- The repository's full test suite must pass in the state you leave behind.

Work through your own discovery of every location carrying this design; do
not stop after the first interface you adjust. The change should read as one
coherent release-preparation refactor of the contract tiering.
