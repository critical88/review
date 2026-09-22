# diff_desc.md — injection design record for pyWhat (bee-san/pyWhat @ 75a1592)

## Maintenance motivation

pyWhat's knowledge lives in two contributor-fed JSON databases shipped inside
the package: `pywhat/Data/regex.json` (the identification patterns) and
`pywhat/Data/file_signatures.json` (file-header signatures). Both are
community-maintained, merged from pull requests, and neither is schema-checked
at load time: the data-access layer reads the raw JSON and hands the records
straight to the identification pipeline.

The injection models the maintenance history such a dataset naturally
accumulates. Every few releases, a contributor-submitted record reaches a
pipeline stage that dereferences a field the record does not carry: a merged
regex entry breaks a consumer, a half-described file signature produces a
half-identified file. The pipeline is long — a record travels from the
distribution pool, through the matcher, into `identify()`'s report, and finally
through whichever output path the user picked — and each incident is triaged by
whoever owns the crashing stage, not by whoever owns the data.

The predictable outcome of that triage pattern is what this diff records: every
stage that consumes records from the shipped databases grew its own local
"is this record complete enough for me" admission check, hard-coding exactly
the fields that particular stage happens to dereference. Each guard is small,
sensible, and defensible in isolation — it was written by a maintainer who
never intended to redefine a shared policy, only to stop their own stage from
crashing.

## Development evolution being modeled

Five small, independent defensive changes, of the kind that land one per bug
report over several months:

1. A report about entries breaking the pool builder and the sorters that
   consume it: the distribution stage bars entries missing the fields it knows
   the downstream filter and sorters read.
2. A report about a malformed match record poisoning the report: the match
   stage refuses entries missing the fields the match record wraps into
   `"Regex Pattern"`, and, because the same incident mentioned a crash inside
   children processing, it also refuses entries whose `Children` configuration
   does not describe how children are matched.
3. A report filed against a `--format` render: the maintainer added the same
   completeness test where the traceback pointed and to the raw output loop
   they happened to be reading; the pretty path was not part of that report
   and received no guard.
4. A half-described file type reaching users: the scan loop skips signature
   records that cannot be matched or described at all.
5. The same file-type incident reviewed by a second contributor, who noticed
   that a *detected* signature still has to survive report assembly and
   rendering: the identifier only stores detected signatures the output layer
   can fully render.

Steps 4 and 5 model the most realistic drift in the sequence: two people
reading the same incident, writing the completeness tests their own stage
needs, and landing them with different field lists. Steps 1–3 model the
slow-burn version: three stages writing the same kind of test for the same
database without coordinating, each with a different field set and different
structure, none of them touching the loader both incident reporters
suspected they should not alter without a broader discussion (the tag
listing and the full test suite already read it).

## Overall design

- Both shipped databases are involved: the regex database through three copies
  (pool, match, render) and the file-signature database through two (scan,
  report assembly).
- The copies disagree where the stages genuinely disagree about the fields
  they read: the pool requires name, rarity and tags; the matcher requires
  name and description plus a valid `Children` block; the renderer requires
  name, description and tags; the scanner requires hex signature and
  description; report assembly requires description and ISO representation.
- The copies cover the pipeline unevenly, the way incident-driven maintenance
  does: pretty printing carries no guard, and the two render-path guards were
  added only where the render bug was reported.
- All decisions are presence-only membership tests
  (`field not in record` / `field in record`), never truthiness, because
  shipped records legitimately carry null values (26 file-signature records
  have a null `"ISO 8859-1"` value; null values pass a presence test but would
  be silently dropped by an `if record[field]` style check written for the
  incident reports).
- Nothing outside the pipeline's production modules changes: no data files, no
  tests, no CLI options, no public signatures.

## Per-cluster rationale

### Regex database — distribution pool (pywhat/filter.py)

**What changed.** A module-level helper decides whether a regex entry may enter
the distribution pool, and the pool builder applies it alongside the existing
rarity/tag eligibility test while building the pool in one comprehension.

**Why this site.** The distribution is the first consumer of every loaded
entry; guarding there means the first incident in the sequence stops at the
gate, which is how the crash was actually reported (the pool builder, and the
rarity/tag sorting machinery that consumes its output).

**Why this shape.** The module already keeps free functions at module scope
(the file is organized as `Filter`, then `Distribution`), so a module-level
predicate next to `Distribution` follows local structure. A plain membership
loop over the three fields the filter and the sorters read is the smallest
possible response to that incident, and it reads naturally in the
comprehension that already had one condition.

**Production role.** Pool admission: nothing enters the distribution's regex
list without name, rarity and tags.

### Regex database — match stage (pywhat/regex_identifier.py)

**What changed.** A `RegexIdentifier` method decides whether an entry may
produce match records, applied as a `continue` guard at the top of the
per-entry loop inside `check()`. Its field requirements extend the pattern: it
requires name and description (the fields the wrapped `"Regex Pattern"`
record is rendered from), and, when an entry carries a `Children`
configuration block, it requires that block to describe how children are
matched.

**Why this site.** The match loop is where an entry becomes a user-visible
result record, and the same incident that motivated the guard involved a
malformed `Children` block: children processing dereferences the block
directly in this same loop. Guarding at the loop head is exactly as far up as
that report needed to reach.

**Why this shape.** A loop with an existing per-entry body needed an
`if not ...: continue` guard at its head to skip cleanly; that reads more
naturally here than wrapping the loop body. The `Children` sub-rule is
expressed on the entry's own dict, next to the code that will later
dereference it, so a reader triaging that crash sees the protection where the
crash was.

**Production role.** Match-time admission, including an independently stated
rule for the optional `Children` sub-record.

### Regex database — output layer (pywhat/printer.py)

**What changed.** A `Printing` method decides whether a matched pattern record
can be rendered as a row, requiring name, description and tags — the fields
the row-building code reads. Two call sites apply it: the per-match loop in
`print_raw` (used by every default `what` invocation) and the per-match loop
in `format_print` (the `--format` substitution path).

**Why these sites.** Both loops build output rows field-by-field from the
record; the guard placement directly before the row construction is where a
half-complete record would otherwise blow up the row mid-print. The pretty
path is left unguarded on purpose: the incident that motivated this cluster
was filed against `--format`, and incident-driven maintenance stops at the
reported path. The asymmetry is the point — the three copies of "which fields
does a record need?" do not even agree on which records they are asked about.

**Why this shape.** The renderer already keeps small private helpers on
`Printing`, so a one-line predicate method mirrors local habit. Callers ask
`self._entry_ok(...)` and `continue`, matching the loop guard idiom used by
the match stage, but re-stating a different field set.

**Production role.** Render-time row admission on the raw and format paths.

### File-signature database — scan loop (pywhat/magic_numbers.py)

**What changed.** A module-level helper decides whether a signature record
from `file_signatures.json` may participate in a scan at all, requiring the
hex signature (what the header is compared against) and the description (what
the result is reported as). The scan loop in `check_magic_nums` skips records
that fail it before dereferencing the hex signature.

**Why this site.** This loop is the only place a signature record becomes a
`re`-comparable probe; a record without a hex signature crashes here first.

**Why this shape.** At this point in the modeled history the signature
database had no consumer-side guards at all, so the response is the smallest
module-scope predicate plus a loop-skip — the same local idiom as the filter
cluster, written independently by a different contributor.

**Production role.** Scan-time admission for file-signature records.

### File-signature database — report assembly (pywhat/identifier.py)

**What changed.** A second module-level helper decides whether a *detected*
signature is complete enough to be stored in `identify()`'s report, requiring
description and the ISO-8859-1 representation. The storage condition in
`Identifier.identify` becomes `if magic_numbers and _signature_entry_ok(magic_numbers)`.

**Why this site.** The report that `identify()` returns is handed directly to
every output path; a stored signature missing the fields renderers read
produces a half-identified file in front of the user. The contributor writing
this guard did not touch the scan loop — they trusted the scan to do its own
checking and guarded the point they owned. That is how the two copies of the
same database's requirements ended up different: the scan-loop copy requires
`Hexadecimal File Signature` + `Description`, while this report-assembly copy
requires `Description` + `ISO 8859-1`, and neither mentions the other's
fields.

**Why this shape.** Joining the helper into an existing `if` condition is the
least invasive way to protect a storage site that already had a truthiness
condition, and keeps the report-assembly code free of the field list.

**Production role.** Result-report admission for file signatures — the last
gate before the record reaches rendering.

## What this record does and does not cover

This record documents production-code edits only; the change consists of the
five clusters above and touches no data files, tests, packaging, or CLI
surface. Whether these edits amount to the maintenance problem they were
modeled after is a conclusion this record deliberately does not draw.
