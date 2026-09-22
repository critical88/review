# Refactoring request: the scan-report pipeline concentrates on a single owner

## Repository

gosec (`github.com/securego/gosec/v2`), a Go static analysis tool. A scan
produces a `ReportInfo` (issues, metrics, suppression info) which the CLI can
print to stdout (`-fmt`) and save to a file (`-out`, `-stdout`, `-verbose`).
Nine output formats are supported: json, yaml, csv, junit-xml, html, text,
sonarqube, golint and sarif.

## Maintainer observation

During a past cleanup of the reporting pipeline we decided that all rendering
should be reachable from one place, so the formatting responsibilities that
used to live in the per-format writer packages were consolidated onto a single
multi-purpose rendering type in the `report` package. That type now owns, at
once:

- format dispatch for all nine formats, including which entry point produces
  which output;
- the policy that decides whether suppressed findings are dropped before
  rendering (all formats except json and sarif drop them);
- the concrete serialization body of almost every format: whole-document
  marshaling, record/column assembly, single-line positional formatting,
  template expansion, and the byte assembly wrapped around the junit and
  sonarqube document builders;
- presentation-only state and helpers: severity color themes, the text
  template function map, code-snippet reconstruction and issue line-range
  parsing, plus the embedded template assets that had moved into that package;
- the presentation ordering of findings (the severity sort and line helpers
  the CLI applies before a report is printed or saved), which has also been
  pulled into that package.

The per-format subpackages under `report/` that still look like the natural
owners of each format no longer contain their behavior; they forward into the
central type. As a result, essentially every output-related pull request —
adding a column, changing a color, touching filtering, fixing a template —
edits the same interconnected method family, and reviewing any one format
means holding the whole pipeline in one's head.

## Diagnosis

The presentation layer violates single responsibility: one type spans the
entire rendering domain. Concretely, these distinct responsibilities are
entangled on the same owner:

1. **Dispatch and policy** — choosing a format and applying the suppression
   rule — is coupled to the implementation of each format.
2. Structurally different serialization families (whole-document marshal,
   record-oriented output, line-oriented output, two kinds of template
   expansion) share one receiver and one state.
3. Presentation-only mechanisms (colorization, snippet reconstruction,
   embedded templates) live on the shared owner instead of with the only
   formats that use them.
4. The finding-ordering step used by the CLI was absorbed into the shared
   owner rather than staying with the reporting path that invokes it.
5. The installed per-format packages became pass-through facades instead of
   owners, so the package layout no longer reflects which code owns what.

## Requested scope

The reporting/output subsystem only:

- the `report` package and the per-format writer packages beneath it
  (`report/json`, `report/yaml`, `report/csv`, `report/golint`,
  `report/html`, `report/text`);
- the reporting entry point(s) they feed and the finding-ordering step the
  CLI performs before a report is written;
- the embedded template assets, which must end up in the directory of the
  package that embeds them.

Out of scope: the scan engine, rules and analyzers, the issue model, and the
`report/sarif`, `report/junit`, `report/sonar` public APIs (their document
builders and `GenerateReport` constructors must not change).

## Desired outcome

Redistribute the pipeline so each responsibility has a cohesive owner:

- Per-format rendering should live again with its format's code (whole
  document, record-oriented, line-oriented, template-driven), together with
  the helpers, assets and colorization that only that format uses.
- What is genuinely shared — dispatch over the supported format names (with
  the existing fall-through to the text rendering for unknown or empty
  formats) and the suppressed-finding policy — should be an explicit, small
  mechanism, not one type that also implements every format.
- Presentation ordering of findings should have a single sensible owner,
  without the CLI duplicating it.
- End the pass-through arrangement: either the per-format packages own their
  implementations again, or the indirection layers are removed, so that the
  package structure and the code that implements each output agree again.

## Behavior and API that must remain stable

- Rendered output bytes for all nine formats must be identical to the
  current output for the same scan input.
- Exported signatures and locations: `report.CreateReport` with its current
  arguments; `WriteReport(w, data)` in the json, yaml, csv, golint and html
  packages; `WriteReport(w, data, enableColor)` in the text package;
  `junit.GenerateReport`, `sonar.GenerateReport` and the sarif package's
  public surface.
- CLI behavior: current `-fmt`, `-color`, `-sort`, `-out`, `-stdout`,
  `-verbose` semantics, including severity-descending finding order and text
  fallback for unrecognized format strings.
- Suppression semantics: suppressed findings are excluded from every format
  except json and sarif.
- The repository's full test suite must remain green without weakening or
  editing the behavioral expectations it currently asserts; tests may move
  alongside the code they lock, but their expectations may not change.
