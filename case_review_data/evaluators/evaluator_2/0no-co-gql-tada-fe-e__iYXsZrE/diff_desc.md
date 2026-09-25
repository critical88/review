# Injection design record — `gql.tada scan` rule analytics

## Maintenance motivation

The `scan` command in the cli-utils workspace package analyses a project's GraphQL
usage: it builds a static module dependency graph for the scanned project, walks
every document definition with a pluggable rule set, and asks each rule to turn
that traversal into datapoints (fields and their reach, query placement, fragment
spread, orphaned definitions, and so on). By construction the pipeline owns one
reusable analytics structure: the module dependency graph class keeps the scanned
project's import map and, by its own stated design, hosts *all* reachability and
area analytics over it — memoised reverse closures, entry points, distances
from entry points, and blast-radius summaries. The rules are meant to ask it for
results; the scan context exposes exactly the graph objects as single primitives
and nothing more.

The realistic pressure on that boundary is time. Rules operate under real
workloads over thousands of files, and the owner's query methods pay a small but
visible cost per call relative to a rule-local computation that a developer can
see hoisted, batched, and reused across an entire statistic (one walk instead of
per-record calls). It is exactly the kind of change that a performance-minded
maintainer writes incrementally, rule by rule, under the impression that each
rule's access pattern is *its own* concern — while in fact each inlined walk
duplicates knowledge the graph class already encapsulates and re-opens the
graph's internal representation to consumers that will have to chase every
future analytics change.

## Modeled evolution

The diff models that incremental evolution after the fact, as if three rule
authors each "optimised" the part of the scan pipeline they owned:

1. The field-usage rule (blast-radius per selected schema field) stops calling
   the graph's per-module reach summary and instead folds the graph's reverse
   closure and area/entry-point facts over the reaching modules, maintaining its
   own union sets in the collection step.
2. The fetch-depth rule (how far queries sit from entry points) stops asking the
   graph for a per-module distance and instead runs its own outward walk from
   the entry points over the graph's raw edge data, keeping a local distance
   table, and then places and labels every query from that table.
3. The cross-feature-fragments rule (fragments consumed across area boundaries)
   resolves the area of the graph's modules up front, eagerly rebuilding a
   full module-to-area table in its setup step and labelling spread sites and
   defining modules from that copy.

All three changes keep output identical, which is precisely what makes such a
drift easy to miss in review: numbers do not move, so the only thing that
regressed is where the analytics knowledge lives.

## Overall design

The injection is confined to the scan pipeline's consumer layer — the rule
implementations that compute graph-derived statistics in their `collect`/`create`
phases. The graph owner itself, the scan context façade, the traversal
machinery, the rule registration table, and the output overviews are all left
exactly as they are: the point of the change surface, and of the task it
poses, is that the owner's public analytics surface is bypassed by consumers,
not that anything about the owner changed.

Each location was chosen because it is a genuine, complete consumer-side
derivation of an existing owner responsibility, not a decorative call site:

- the blast-radius rule needs the unioned reach of *sets* of modules, which the
  owner only offers per module — a batch-shaped gap that invites exactly the
  inline fold a time-pressed author writes;
- the placement rule needs a distance for *every* query module in one pass,
  which invites hoisting the owner's memoised walk behind a local table;
- the cross-feature rule labels areas at *every* spread site and for every
  defining module, which invites an eager local index over the owner's edge
  data.

In each case the owner already provides the computation being duplicated
(per-module reach, per-module distance, per-module area), so nothing new was
invented on the consumer side — the analytics were re-hosted, not redesigned.

## Per-location rationale

### 1. Field blast-radius aggregation (field-usage rule, collection step)

**What changed.** The collection step's blast-radius computation previously
asked the graph for a reach summary per source module and merged its three
parts. It now expands the reach itself: for every source module it drains the
graph's dependents closure into a local module set, seeds the set, and then, in
a second pass over that local set, collects areas through the graph's area
mapping and checks entry-point membership against the graph's entry-point set.

**Why this site.** This is the pipeline's most heavily used statistic (the
field-usage rule indexes every selected schema coordinate and is the backbone of
the terminal overview), so it is the most plausible place for a
performance-motivated rewrite, and it is the rule whose input is naturally a
*set* of modules — the strongest local justification for writing a set-level
fold instead of per-module owner calls.

**Why this form.** The fold keeps the rule's downstream data contract
untouched: the three local sets (modules, areas, entry points) are exactly the
fields the datapoint already published, so the change reads as a pure
"compute these ourselves" rewrite with no API impact. The two-pass shape also
mirrors how a developer rationalises the change — expand first, annotate
second — rather than a wrapped call.

**Production role.** Blast radius is the headline weight for field usage when
the scan output ranks which schema coordinates matter; this rule drives the
"which fields does the codebase actually depend on" reading of the scan.

### 2. Entry-distance placement (fetch-depth rule, collection step)

**What changed.** The collection step previously placed each query by a single
owner call for its module's entry-point distance and labelled the datapoint with
the owner's area mapping. It now runs the placement walk itself: it seeds a
frontier from the graph's entry points, walks outward over the graph's raw edge
map while building a local distance table, and then, in a single fold over the
traversed operations, looks each query module up in that table and labels it
with the per-module area fetched from the graph.

**Why this site.** Fetch depth is the pipeline's only statistic that needs a
distance for *every* module at once, so the "hoist the walk" reasoning is most
credible here; it is also a self-contained rule with an empty visitor, making it
the natural second place a maintainer would touch after field usage.

**Why this form.** The walk mirrors the owner's own algorithm shape (frontier,
per-level counter, skip-if-seen) because that is what an author copies when
moving logic down a level; the tail is written as a fold over operations so the
labels and messages are built in one pass. Keeping the message text, weights,
null-depth semantics and sort order identical to the previous form preserves
every downstream consumer of the datapoint verbatim.

**Production role.** The depth distribution describes data-fetching placement
in the scanned app (hoisted queries near route boundaries versus waterfalls deep
in the tree) — it is the placement lens of the scan output.

### 3. Eager module-to-area indexing (cross-feature-fragments rule, setup step)

**What changed.** The rule's setup step previously resolved a module's area by
asking the graph at each use — the visitor labelling each spread site and the
collection fold naming a fragment's defining area each called the owner
directly. The setup step now eagerly builds a complete local module-to-area
table by iterating the graph's raw edge map (key modules and their import
targets), and both the visitor closure and the collection fold read from that
local copy, keeping an owner call only as a fallback for modules the table does
not carry.

**Why this site.** The cross-feature rule needs area labels at spread-site
granularity, so an author wanting "one lookup" has an easy story for
pre-flattening the owner's edge data into a table. Its visitor runs on every
fragment spread in the scanned project, which makes a setup-time index look
like a real win over repeated owner calls during traversal.

**Why this form.** The table is built in the setup step (`create`), alongside
the rule's other traversal-time state, and consumed from both the visitor
closure and the collection fold — the eager/lazy fallback shape is retained to
stay total over modules that never appear as graph nodes. This keeps the
rule's handling of non-node modules (which can arise for documents at the
scanned project's roots) unchanged in behaviour.

**Production role.** The cross-feature view is the pipeline's coupling signal:
which fragments act as seams between areas of the codebase, used when teams
look for feature boundaries to untangle.

## Behavior considerations

The injection is a re-hosting of existing computations, not a redesign: per
field, the unioned reach is unchanged; per query, the distance table computed
by the local walk is the same table the owner's walk memoises, and queries with
no path from an entry point keep reporting an unknown depth with the same
wording and the same descending ordering; per fragment, the local area table
stores exactly the owner's mapping and falls back to the owner, so labels are
identical in every case. Messages, weights, data fields, datapoint ordering and
the rule's ranking behaviour are all preserved by construction, so the scan
command's output — terminal, json, and graph output alike — does not move.
