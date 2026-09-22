# Consolidate pyWhat's duplicated database-record admission policy

pyWhat identifies what a text, a file, or a directory contains. Its knowledge
lives in two contributor-fed JSON databases shipped inside the package
(under `pywhat/Data/`): the identification patterns and the file-header
signatures. Neither database is schema-validated anywhere; the module that
loads them reads the raw JSON and hands records straight to the rest of the
pipeline. From there a record travels through the distribution pool and the
regex matcher, an optional file-signature scan, the report assembled by the
identifier, and finally through whichever output path the user selected
(default text rows, `--format` substitution, or `--json`).

## The observation

Over the past several releases, each consuming stage of those databases has
acquired its own local "is this record complete enough for my stage" test.
None of them was planned: every one landed as the immediate response to a bug
report about a record that broke one stage, written by whoever triaged that
particular crash. The results are exactly what you would expect:

- The same decision — "which fields must a shipped record carry before the
  pipeline may trust it?" — is re-stated independently by several stages that
  consume the databases.
- The copies disagree. The distribution pool expects one field list, the match
  stage a different one (plus its own idea of what a valid `Children` block on
  a pattern entry looks like), the output rows yet another. On the
  file-signature side, the code that scans headers and the code that assembles
  the report enforce different requirements for the *same* record; a record
  can satisfy one and not the other.
- Coverage is uneven and silent. Some consumption paths re-test records that
  other paths already admitted, and at least one rendering path never got a
  copy of "its" test at all. Whether a shipped record survives the pipeline
  depends on which stages happen to have been taught about it.
- When we need to change the rule — for example, when we decide pattern
  entries must start carrying another field, or when contributor tooling
  learns to pre-validate submissions — we have no single place to edit. The
  decision lives in whichever stages currently carry a copy, and agreeing to
  a requirements change means finding and editing all of them without a map.

This is a textbook shotgun surgery problem: one conceptual responsibility is
scattered across several implementations, so every change to it is a small
coordinated edit in many places, and the separate copies drift apart without
anyone noticing.

## What we want

Give pyWhat's database-record admission policy one authoritative definition
and stop re-deriving it per stage:

- Define, in one place, the rule that decides whether a record from each of
  the two shipped databases is complete enough to be used. The two databases
  have different schemas, so their requirements must be stated individually
  rather than flattened into one generic list.
- Enforce the rule where those databases are read — the data-access layer
  that loads them is the natural owner — so every consuming stage receives
  records that already satisfy it, instead of re-screening them locally.
- Remove the stage-local copies of the rule: consuming code (pool building,
  matching, file-signature scanning, report assembly, the output paths) should
  no longer carry its own field-presence tests for these database records.
- Leave the pipeline able to reject records that genuinely are half-formed:
  a contributor-submitted record missing required fields must still never
  reach rendering, at any stage, with the same or better guarantees than
  today.

A repair that only renames, deduplicates, or slightly reorganizes the
existing per-stage tests into a shared bag of helpers — while each stage keeps
deciding admission for itself — is not what we mean: areas that currently
carry their own copy of the decision should end up trusting records validated
at the boundary instead.

## What must stay stable

- User-visible behavior on the shipped databases is unchanged: text and
  directory scanning, `--json` output, `--format` (including `pretty`),
  `--rarity` ranges, `--tags`/`--include`/`--exclude`, the boundaryless
  options, key sorting (e.g. `-k rarity`), and the `--tags` listing must
  produce their current output.
- Admitted records stay admitted: the databases contain records whose
  optional field *values* are legitimately empty or null (for example,
  file signatures whose `"ISO 8859-1"` value is null). Those records are
  shipped data; presence of a field, not truthiness of its value, decides
  completeness. Dropping any record the pipeline currently admits would be a
  regression.
- The existing unit tests keep passing without modification.
- The public surface keeps its shape: `Identifier.identify`, the classifier
  entry points (`what.py` / `python -m pywhat`), and the report structure the
  CLI and tests rely on ("Matched" / "Regex Pattern" records, the
  "File Signatures" and "Regexes" report sections) must not change.
- No changes to the shipped JSON data files and no new third-party
  dependencies.

## Out of scope

The matching, filtering, and rendering logic itself is fine and should not be
redesigned; the rarity/tag *matching* window the distribution applies to
already-valid entries (the `Filter` semantics) is a user feature, not a
completeness test, and must survive the refactor as-is.
