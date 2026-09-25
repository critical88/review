# diff_desc.md

## Maintenance motivation

golink resolves short names — the `foo` in `http://go/foo` — through two related
steps. First the server canonicalizes a name into a **storage identity** (the
SQLite row ID), so `Foo`, `foo`, and `B-c` map to one row. Second, because users
type names from memory and paste them out of documents and chat, the server
reconciles messy input — wrong case, surrounding whitespace, stray trailing
punctuation — before it reports "link not found". These two steps share one
underlying rule (lower-case the name, drop dashes, and optionally trim trailing
punctuation), and the rule is exercised at many boundaries: every SQLite
access, four HTTP handlers, and the `--resolve-from-backup` command-line path.

Over a series of small, individually-reasonable maintenance changes the code
accumulated per-boundary inlined copies of that rule. The storage layer grew a
single `linkID` helper, but then individual accessors began spelling the same
computation out again in slightly different idioms. When case-insensitive
matching was retrofitted onto the HTTP handlers, each handler folded/trimmed
the incoming name in its own block rather than going through a shared helper.
Splitting `resolveLink` into its own file carried the resolver's local copy
along with it. The natural development evolution being modeled is therefore:
"new boundary added -> maintainer inlines the tolerance 'just for this path'
-> next boundary does the same" — exactly how scattered responsibilities
usually accrue on a real project, where no single change is obviously wrong
but the conceptual rule is now in many places.

This record documents the design of those changes: which sites were edited,
why that site and implementation shape were selected, and the production role
each serves. It is auditable design rationale; it makes no claim about whether
the changes constitute a single smell or about what the right repair is. Those
are assessment conclusions and are not part of this record.

## Overall design

The injection spreads the shared short-name identity/tolerance rule across
three files in the production tree, organized by production role:

1. **Storage layer (`db.go`)** — the single identity helper is removed and each
   `SQLiteDB` accessor that needs a row ID recomputes it inline, in a couple of
   idiomatic variations.
2. **HTTP handlers (`golink.go`)** — four handlers (redirect, detail, delete,
   save) each gain a local block that reconciles messy input before lookup, and
   the conflict-detection check in `serveSave` gains a local fold of the name.
3. **Resolver (`resolve.go`, new file)** — `resolveLink` is split out of the
   main server file (it grew a caller on the command line) and carries its own
   local tolerance block for the CLI path.

Splitting `resolveLink` into its own file is motivated independently (the
function now has a command-line caller, so it was extracted for reuse) and is
the kind of move a maintainer makes in the same change set as the tolerance
retrofit; it also carries the resolver's local copy of the rule across the new
file boundary.

## Per-cluster design

### Storage layer (`db.go`)

The original accessor set (`Load`, `Save`, `Delete`, `LoadStats`, `SaveStats`,
`DeleteStats`) all obtained their row ID from one shared helper. In the
injected state that helper is removed and each accessor re-derives the ID:

- `Load` and `Delete` spell the computation as a single inline expression in the
  SQL call argument. This is the smallest, lowest-friction shape a maintainer
  reaches for: "I already have `short` right here, just compute it inline." The
  expression `strings.ReplaceAll(url.PathEscape(strings.ToLower(short)), "-",
  "")` reads right-to-left as the obvious composition of the three known rules.
- `Save` introduces a named local (`esc`) and builds the ID in two statements.
  This shape was selected because `Save` already declares several locals and is
  the most read/edited accessor, so a maintainer here tends toward the slightly
  more verbose, debuggable form rather than the one-liner.
- `DeleteStats` mirrors `Load`'s one-liner.
- `SaveStats` uses `strings.NewReplacer("-", "")` instead of
  `strings.ReplaceAll(..., "-", "")`, a genuinely different idiom for the same
  dash-stripping rule. This was selected deliberately to model drift: different
  maintainers, or the same maintainer at different times, reach for different
  stdlib spellings of "strip these runes", so the shared rule is no longer even
  *syntactically* identical across sites — a change to how dashes are handled
  would have to be applied in multiple forms.
- `LoadStats` builds the ID-to-short lookup map with a *partial* copy of the
  rule: it lower-cases and strips dashes but omits percent escaping
  (`strings.ReplaceAll(strings.ToLower(link.Short), "-", "")`). This was
  selected to model the most insidious drift — a variant that is *almost* but
  not quite the canonical rule, correct only for the subset of names that
  contain no characters `url.PathEscape` would change. Any future change to
  escaping would have to be remembered here too.

Net effect: the storage layer now owns the row-ID rule in six places (one per
accessor), in three idiomatic shapes.

### HTTP handlers (`golink.go`)

When case-insensitive matching and paste tolerance were retrofitted onto the
handlers, each handler grew its own pre-lookup reconciliation block instead of
calling a shared normalizer:

- `serveGo` (the redirect path) already had a trailing-punctuation fallback.
  It gains a *second* fallback stage that case-folds the name and retries the
  load. This two-stage shape was selected because it layers naturalistically on
  top of the existing fallback: a maintainer adding "also try lower-case"
  appends a stage to the existing `if errors.Is(err, fs.ErrNotExist)` block
  rather than refactoring the block to call a helper. Result: `serveGo` holds
  two distinct inline reconciliation primitives.
- `serveDetail` previously only stripped the `/.detail/` routing prefix; it now
  also folds case and trims a trailing-punctuation set from the raw path
  segment before `db.Load`. Shaped as one expression so the folding is visible
  at the call.
- `serveDelete` does the same, additionally trimming surrounding whitespace
  (deletes arrive from form fields that can carry stray spaces). The
  whitespace trim is hygiene specific to the delete form, layered on the same
  case/punctuation fold.
- `serveSave` adds a conflict check: before accepting a new link it folds the
  incoming name to a "does this shadow something?" key and loads that key. The
  fold here (`strings.ReplaceAll(strings.ToLower(short), "-", "")`) re-derives
  the *storage identity* rule (not the request-side tolerance rule) inside the
  handler, so a name is folded one way for conflict detection and another for
  tolerance — both inline, both in this file.

These four edits were selected because each sits at a distinct surface where a
user supplies a name, so each is a plausible place for a maintainer to type "be
forgiving about input here". They are also spread across the file rather than
clustered, so a reader looking for "where names get folded" finds the work
repeated in several neighborhoods.

### Resolver (`resolve.go`, new file)

`resolveLink` already existed in `golink.go`; it is extracted to a new
`resolve.go` because a command-line path (`--resolve-from-backup`) began calling
it, and extraction for reuse is the normal refactor a maintainer performs in
that situation. The extracted copy carries a local tolerance block —
`strings.TrimRight(strings.ToLower(short), ".,()[]{}")` before `db.Load` — so
the CLI path applies the same reconciliation the HTTP paths do, spelled out
again in this file.

This edit was selected to (a) place a genuine shotgun-surgery site in a
separate file, increasing the change's file-spread, and (b) model the
realistic step where moving a function to its own file also moves the
function's local copy of the rule across the file boundary, so the rule now
lives in `db.go`, `golink.go`, **and** `resolve.go` simultaneously. The
extraction itself is legitimate and beneficial (reuse on the command line);
what it transports is the unsolved duplication.

## Summary of the modeled evolution

A single conceptual rule — lower-case a short name, drop its dashes, optionally
strip trailing punctuation, and escape it for storage — was, over a series of
small maintenance changes, re-spelled in many places: six storage accessors in
three idioms (including one partial copy and one alternate stdlib form), four
HTTP handlers each with their own reconciliation block, and a resolver
extracted to its own file with its own copy. No individual edit is absurd; the
diff is a plausible artifact of accrued maintenance. Whether these edits
amount to one consolidated responsibility that has been scattered, and whether
the right response is to re-centralize it, is left for downstream assessment.
