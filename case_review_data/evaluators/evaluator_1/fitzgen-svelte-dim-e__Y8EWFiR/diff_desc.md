# Injection design record — Deeply Inlined Method

This is the auditable design rationale for the injected change, not private
chain-of-thought. It records the realistic maintenance motivation, the normal
development evolution being modeled, the overall design, and a per-cluster
explanation of what changed and why a given site and implementation shape were
chosen. It does not prescribe a repair and draws no conclusions about whether
the change amounts to a single smell or about the presence of unrelated work;
those are left to separate review.

## Maintenance motivation

`twiggy analyze` exposes each size analysis as a thin entrypoint that delegates
to a set of small, focused helpers — one helper per pipeline stage (collecting
inputs, gating them by options, computing a derived number, and summarizing for
display). A maintainer working on a bug or a performance question in one
analysis frequently has to step through that whole helper chain to follow how
option flags interact with the final summary rows. The pattern modeled here is
the locally-defensible, "I want to see everything in one place" flattening that
happens during that kind of focused investigation: the maintainer copies a
helper's body into its only caller so the whole pipeline reads top-to-bottom,
deferring the re-decomposition. Each collapse is reasonable in isolation;
collectively the per-stage boundaries that a future maintainer relies on are
erased.

## Normal development evolution modeled

The change models the slow collapse that accumulates during ordinary feature
work rather than a single rewrite. A helper that has exactly one caller has its
body copied into that caller while a fix is being tried. Helpers that grew
under now-stable option flags get merged back into the caller as the flags
settle. A helper shared with another module gets inlined into a local caller so
a local edit can stay self-contained. None of these steps is presented as a
rewrite; they are the kind of edits that survive a hurried commit and then
become the region's normal shape.

## Overall design

Three analysis entrypoints are collapsed, each a distinct pipeline that
previously delegated to per-stage helpers. The collection/gating mechanics are
rewritten from iterator-closure style into explicit loop accumulators so the
inlined body reads as straight-line code rather than as obviously extracted
blocks; control flow that was previously a closure's body is now nested
statements owned by the entrypoint. Deduplication structures (the BTreeSet for
collapsing duplicate monomorphizations, the sort orders) are kept because they
encode observable output ordering, but their use is moved inline.
Behavior — the exact bytes emitted by each analysis for every option
combination — is preserved by construction and is otherwise left unchanged.

## Cluster: the monomorphization analysis (`analyze/analyses/monos/mod.rs`)

`monos` previously delegated to a six-stage chain: collecting generic
monomorphizations, computing per-generic total/bloat, truncating the retained
instantiations by options, appending a "… and N more" row, and the totals row.
The injected `monos` performs all of these inline. The fold-based collection
and summarization are rewritten as explicit `mut` accumulator loops so the
per-stage logic reads as one body; the BTreeSet dedup and the size-descending
then name-ascending sort are retained because they determine the displayed
order. This site was chosen because it has the deepest chain and the most option
interaction (only-generics, max-monos, max-generics), so it is the natural
first target when a maintainer is trying to understand the analysis end-to-end,
and because inlining it produces a body whose nested collection, gating, and
summarization visibly mix several abstraction levels.

## Cluster: the dominator-tree analysis (`analyze/analyses/dominators/mod.rs`)

`dominators` previously delegated the unreachable-items summary to a private
helper, which in turn reached across into the `garbage` analysis for the
reachability walk. The injected `dominators` keeps the dominator-item
selection inline (as it already was) and additionally folds in the reachability
DFS from the meta root and the unreachable-items summary, including its
gating on "no explicit items were named" and "there is at least one unreachable
byte." Because the reachability walk is shared with the `garbage` analysis, this
is a cross-module responsibility being absorbed into a local caller — the kind
of inlining that makes a single analysis self-contained at the cost of
duplicating a walk that another analysis still needs. This site was chosen to
demonstrate an inlined region that spans another analysis's responsibility, so
the collapsing boundary is not merely within one file.

## Cluster: the retaining-paths analysis (`analyze/analyses/paths/mod.rs`)

`paths` previously delegated starting-position resolution to a helper that held
four sub-closures — regex match, exact-name match, default ascending, and
default descending — plus an option-driven dispatch over them; the recursive
entry-creation walk stayed in its own helper. The injected `paths` folds the
four resolution branches and their dispatch directly into the entrypoint as
explicit `if`/`else if` blocks with inline loops, while the recursive walk is
left intact because it is genuinely recursive and not the locus of the collapse.
This site was chosen because the four-closure helper is a single mobile stage
that is easy to inline in a credible "I'll just put this logic here" edit, and
because its collapse introduces option-driven branching (regex vs exact,
ascending vs descending) directly into orchestration rather than delegating it.

## Per-site shape rationale

Across the three clusters the inlined regions are deliberately intermixed with
the entrypoints' pre-existing orchestration (compute-dominator-tree calls,
compute-predecessors, the `Box<dyn Emit>` construction) rather than appended as
a sequential block, so the collapsed stages sit at different nesting depths and
the bodies are not separable by a single top-level split.
