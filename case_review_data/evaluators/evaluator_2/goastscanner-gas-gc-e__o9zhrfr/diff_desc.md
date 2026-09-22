# Change record: consolidating scan-report rendering into one coordinator

## Maintenance motivation

gosec ships nine output formats (json, yaml, csv, junit-xml, html, text,
sonarqube, golint, sarif), and each one grew up in its own package under
`report/`: `report/json`, `report/yaml`, `report/csv`, `report/golint`,
`report/html`, `report/text`, plus the builder-style `report/junit`,
`report/sonar` and `report/sarif` subsystems. Dispatch lived in one place
(`report/formatter.go`), but everything a format actually does — traversing
findings, formatting columns, parsing issue line ranges, applying severity
colors — was duplicated or scattered: every writer opened its own loop, the
text writer carried a private family of helpers (`plainTextFuncMap`,
`highlight`, `printCodeSnippet`, `parseLine`) plus three package-level color
themes, and suppressed-finding filtering had to be remembered after
`CreateReport` chose a writer.

The recurring friction reported by maintainers: adding one capability to
reporting (a new format knob, a shared suppression rule, better code
highlighting) required editing up to a dozen files across packages, each with
a slightly different signature convention — the text writer even takes an
extra `enableColor` argument the other formats do not. The decision taken here
is to consolidate: give the report tree a single rendering object that owns a
finding stream end to end, and reduce the per-format packages to thin
compatibility entries so existing callers (the CLI and downstream importers)
keep compiling unchanged.

## Normal development evolution being modeled

This is the classic "integration convenience grows into a hub" evolution:
a coordinator type is introduced to unify entry points, then gains the
suppression policy because "it already touches every render", then absorbs the
per-format bodies one by one whenever a format needs a tweak, then inherits the
text helper family, the color themes, the embedded templates and finally the
presentation ordering previously owned by the CLI. Every step is locally
motivated by "make the pipeline reachable from one place"; no caller-facing
behavior changes along the way, which is exactly why nothing pushes back
against the growth.

## Overall design

The new `report.Renderer` type (package `report`) is built by `NewRenderer`
and a small builder chain (`forFormat`, `withColor`, `withRootPaths`) so each
output entry passes only the knobs it needs. `Render()` dispatches on the
format string exactly as `CreateReport` did before, with the same fall-through
to text rendering for unknown or empty formats. Suppressed findings are dropped
once, up front, for every format except json and sarif (which intentionally
retain them for audit tooling). The per-format `WriteReport` functions remain
exported in their original packages as forwarding entries; the ordering
comparator used by the CLI (`sortIssues` / `extractLineNumber`) now delegates
into the report package, keeping both helper names in `package main` intact for
its callers.

Behavior is preserved: identical rendered bytes for the same input across all
nine formats, identical CLI flag semantics, and unchanged public signatures for
`report.CreateReport`, every `report/<format>.WriteReport`, the junit/sonar
builders and the whole sarif subsystem.

## Per-cluster changes and rationale

### `report/renderer.go` — coordinator core

New file holding the type definition, `NewRenderer` factory, the builder
chain, `CreateReport` (now a thin wrapper that wires the builder chain), the
`Render` dispatch switch, and two pipeline pieces: the suppression policy and
the ordering helpers absorbed from the CLI.

- Why this site: `CreateReport`'s switch was already de-facto the pipeline's
  hub; moving it onto the new type keeps a single decision point for which
  rendering runs, and lets the suppression and ordering steps share the type's
  state instead of relying on callers to sequence them. Because the policy now
  runs inside `Render` before dispatch, no caller can obtain a rendering
  without also obtaining the filtering step — one decision, applied
  everywhere, which is exactly what made consolidation attractive here.
- Why this shape: a builder chain (`forFormat`/`withColor`/`withRootPaths`)
  absorbs the two signature irregularities — only text consumes
  `enableColor`, only sarif consumes root paths — without forcing every entry
  point to take every parameter.
- Production role: the entry every caller reaches; owns the whole
  before-render pipeline (suppress, order) and the format decision.

### `report/renderer_data.go` — byte-stream and record serializers

New file with the rendering bodies for the whole-document marshal formats
(json, yaml), the record-oriented csv writer with its per-finding row
assembly, and the golint single-line printer with its CWE labeling and line
computation.

- Why this site: these formats share the "walk findings, produce bytes"
  skeleton; joining them lets one small `emit` helper own write-error handling
  for all of them and keeps csv's column list next to golint's field order,
  which both encode the reporting contract of a finding.
- Production role: per-format serialization of a `ReportInfo` into a byte
  stream, including the exported `WriteJSON`, `WriteYAML`, `WriteCSV`,
  `WriteGolint` compatibility functions used by the shims below.

### `report/renderer_templates.go` — template-driven formats and color schemes

New file with the text and html rendering bodies, the two template parse
helpers, the text function map, the severity color themes, the
code-snippet printer and the issue line-range parser absorbed from
`report/text/writer.go`.

- Why this site: the text format is the only one with a function map,
  colorization and a code-snippet renderer, and html shares nothing with it
  except the template mechanism; keeping the pair together means the
  highlighting and snippet logic has exactly one owner after the move.
- Why this shape: the color themes and func map move from package-level
  variables onto an `initColorSchemes` method so that highlighting and
  rendering read from the same object rather than split state; snippet
  reconstruction (`renderCodeSnippet`, `issueCodeRange`) keeps the original
  byte-for-byte output, including the exact indentation and marker strings.
- Production role: presentation-only rendering: colorize by severity, render
  code context for findings, expand both embedded templates.

### `report/renderer_reports.go` — builder-backed outputs

New file with the junit-xml, sonarqube and sarif rendering bodies. The junit
and sonar **document builders are intentionally not absorbed**: `junit.GenerateReport`
and `sonar.GenerateReport` with their exported types stay where they are,
because their packages own the report models. This cluster only owns the byte
assembly around them — indented marshaling, the xml header, stream writing.
For sarif, the coordinator wires the existing writer, which is why the builder
chain has a `withRootPaths` stage.

- Why this site: assembling the final bytes was previously the outer half of
  the dispatch switch; centralizing it keeps the "everything ends in a stream
  write" invariant in one type while leaving the deeper report models alone.
- Production role: the last mile for the three builder-style formats.

### Per-format writer packages — compatibility shims

`report/csv/writer.go`, `report/golint/writer.go`, `report/html/writer.go`,
`report/json/writer.go`, `report/text/writer.go`, `report/yaml/writer.go`
shrink to one-line `WriteReport` entries forwarding into the coordinator
(`report.WriteCSV`, `WriteGolint`, ..., with text passing along
`enableColor`).

- Why this site: the exported API of each package (used by the CLI and by
  importers of gosec as a library) must not change signature or location;
  keeping the entries means no caller updates and any later format tweak can
  still be found from the old entry point.
- Production role: stable facade per format package.

### `report/formatter.go` — removed

The old dispatch hub is deleted: `CreateReport` and `filterOutSuppressedIssues`
move onto the coordinator (`renderer.go`), since dispatch and the suppression
policy now live with the type that applies them. Removing the file prevents a
second dispatch path from surviving next to the new one.

### Template assets — relocation into `report/`

`report/text/template.txt` and `report/html/template.html` move to
`report/template.txt` and `report/template.html`, byte-identical. This is a
mechanical requirement of `go:embed`: an embedded asset must live in the
directory of the package that embeds it, and the embedding moved into the
coordinator's file group. The template content itself is untouched; the
rendering output is unchanged.

### `cmd/gosec/sort_issues.go` — ordering absorbed from the CLI

The severity/presentation comparator used by `gosec` before printing or saving
a report moves to the report package (`OrderIssuesForReport` and
`IssueLeadingLine`); the CLI's `sortIssues` and `extractLineNumber` remain as
one-line delegates.

- Why this site: ordering findings before a report is presentation work, and
  the coordinator now owns presentation; keeping the thin delegates preserves
  the CLI helper names used inside `package main`.
- Production role: presentation ordering shared by the report path and the
  CLI.

## Why the remaining report-tree code stays put

`report/sarif` keeps its generator, builders, schema types and writer: it is
a cohesive, self-contained subsystem with a public API that does not share the
byte-stream skeleton above. `report/junit` and `report/sonar` keep their
report models and `GenerateReport` constructors for the same reason — only
their output stage is shared. The issue model, CWE data, root-package caches
and filters are scan-time constructs rather than presentation, and pulling
them upward across the existing import direction would create cycles.
