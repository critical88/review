# Injection-design record — deeply folded helper subtrees in rust-algorithms

## Maintenance motivation

This crate is a competitive-programming library: per-call constant factors are
its selling point, and its algorithms are documented in the literature as a
driver plus a small set of named sub-procedures. The change models a familiar
degeneration path in exactly such a code base.

During a tuning pass, a maintainer stops routing hot paths through the little
helper layer: each indirection is one more thing the optimizer has to see
through, each helper body is short enough to paste, and once the body is inside
the caller it can be specialized to that call site (a bound tightened, a
redundant branch dropped, a temporary assigned to a named local). The helpers
themselves then lose their last callers and are deleted, because a private unit
with no callers is dead weight in a crate that prizes leanness. Nothing in the
process is a single dramatic decision: each absorption is locally defensible —
faster to read top-to-bottom, one less symbol to chase — and the crate keeps
passing its whole corpus. The cumulative result is that the library's most
important entry points now each carry several layers of the pipeline by hand,
leaf arithmetic repeats next to top-level traversal, and the named stages that
matched the textbook description of each algorithm no longer exist as callable
units.

## Normal evolution being modeled

The diff represents one such tuning-and-lean-out session over four files:

1. **Hot-path specialization.** The range-query tree paths and the factoring
   driver absorb the bodies they used to call so each call site can be
   hand-specialized; the now caller-less private helpers are removed.
2. **Self-contained procedures.** The suffix-array constructor re-derives its
   ordering stage inline at each application, on the theory that a reader
   following one pass should not be sent to another file.
3. **Renamed and localized bookkeeping.** Wherever a pasted body met its new
   surroundings, the boundary got flattened: guard conditions merged, layer
   parameters became locals named for the surrounding narrative, and comments
   started describing "phases" of one continuous procedure instead of the
   former division of labor.

## Overall design of the change

Five algorithmic entry points were enlarged by folding their private call
subtrees into their own scopes, and the corresponding private helpers were
removed from their modules. All five absorptions follow the same recipe — paste
the callee bodies at every former call site, then adapt each paste to its new
surroundings — which is what makes the result hard to read afterwards: the
pasted fragments are *not kept verbatim*. The adaptations are deliberate and
intended to look like ordinary site-specific tuning:

- **Renamed intermediates.** Parameters of the removed helpers reappear as
  locals named after the absorbing function's own narrative (`left_child` /
  `right_child` for a dispersal step, `dead_region` / `saturated` for overlap
  regimes, `clone_slot` for an arena append).
- **Guard-merging and re-branching.** The primality screen keeps its old
  early-exit structure only in shape; early returns became labeled-block
  breaks inside the driver's work-list loop, because plain `return` would now
  mean something different at the outer level.
- **Different arithmetical idiom at the leaves.** The removed modular helpers
  computed `a * b mod m` in reduced form; the pasted sites spell the same
  computation as a 128-bit intermediate plus a non-negativity fix-up local.
  Values agree at every step, but the standing idiom of the module changed.
- **Defensive residue.** Some fix-up branches that the removed helper
  guaranteed once survive at paste sites even where the surrounding values
  make them unreachable — exactly what happens when a leaf is inlined under
  time pressure.
- **Phase narration.** Each absorbing body carries comments presenting its
  content as consecutive phases of one procedure, proposing decomposition seams
  that match the paste boundaries rather than the original division of labor.

The absorbed logic originates in five source files across three algorithm
domains (number theory in `src/math/mod.rs` and `src/math/num.rs`, string
processing in `src/string_proc.rs`, and range queries in
`src/range_query/static_arq.rs` and `src/range_query/dynamic_arq.rs`); the
removal of the helper units themselves touches the four files listed below.

## Per-cluster record

### 1. `src/math/mod.rs` — factoring driver absorbs screening and divisor search

**What changed.** The factoring entry point's work-list loop now performs the
whole pipeline itself: the deterministic 12-base composite screen (bases
constant, odd-part split, square-and-multiply loop, squaring chain with early
witness exits) and the parameterized cycle-based divisor walk (tortoise/hare
steps on `x^2 + a`, periodicity check, an inlined Euclid reduction at the
detection point). The dedicated primality-screen and divisor-search units that
lived in this module were removed; the Euclid reduction absorbed here is the
same standing idiom as the `fast_gcd` unit that `src/math/num.rs` still keeps
for the rational-number constructors, so the module no longer has a single
canonical copy of that idiom.

The modular-arithmetic leaves (128-bit product, positive remainder) are written
out at every former call point rather than routed through the module's small
modulo helpers, although those helpers remain in the file for the primality
test's own path.

**Why this site and form.** This is the deepest call chain available in the
crate: screening, exponentiation and reduction leaves, plus a separate search
walk and a cross-file gcd idiom. Folding it exercises the largest genuine
subtree the repository offers. The labeled-block form was forced by the site:
the screen used to `return` from a small unit, but pasted inside a work-list
loop an early exit has to break the outer iteration, which is precisely the
kind of control-flow damage such a fold produces. The defensive fix-up
branches were kept at the paste sites as part of the tuning-session story.

**Production role.** The factoring pipeline is the number-theory workhorse of
the crate and the anchor of its documented behavior for large `i64` inputs.

### 2. `src/string_proc.rs` — suffix-array constructor absorbs its ordering stage

**What changed.** The constructor performs its stable counting order (tally
into a key-space array, running starts, scatter) twice inside its own body:
once over the byte alphabet for the initial order, and once inside the
rank-doubling loop over the previous round's rank keys, with the second pass
using re-usable `clone`d ranges just as the original loop did. The module's
shared ordering helper no longer exists.

**Why this site and form.** This is the only site in the crate where the same
sub-procedure is used at two different key spaces *and* is the natural
"helper-to-paste" candidate during a lean-out: its body is seven statements,
its parameters are just key-array accesses, and a reader of the doubling loop
can follow a single inline pass without flipping to the helper. Keeping the
first application inline as well is what one realistically ends up with: after
the second fold, the helper has one remaining caller, too few to justify the
symbol.

**Production role.** This constructor builds the crate's central text-indexing
structure; its stability contract is what makes the doubling loop correct.

### 3. `src/range_query/static_arq.rs` — both tree paths absorb propagation and aggregation bookkeeping

**What changed.** Both the range-apply walk and the range-aggregate walk
unroll the former level-by-level pending-tag dispersal unit at each range
border (a border-conditional application on one side, an unconditional one on
the other), writing out the child addresses, leaf-count scoping and per-child
compose bookkeeping at every step. The update walk additionally absorbs the
per-node value application and tag compose at both cover sites (total-overlap
guards become local `if size > 1` checks) and re-aggregates ancestors with two
inline reduction loops instead of the former aggregation helper. The shared
dispersal/aggregation units were removed from the type.

**Why this site and form.** The two walks are the write and read paths of one
component sharing the same propagation discipline, so this cluster shows the
same absorption appearing on both sides of a read/write boundary with
site-specific adaptations (the update cover uses the walk's running leaf
count; the aggregate side does not, so its paste is the dispersal part only).
The flat-array tree follows the compact Al.Cash representation where a node
index and its computed leaf count are the whole "node object", which makes the
inlined bookkeeping index-arithmetic-heavy rather than pointer-heavy.

**Production role.** This is the crate's workhorse structure for fast range
updates and range aggregates, exercised by the leftmost-negative example walk
that survives unchanged in this file.

### 4. `src/range_query/dynamic_arq.rs` — persistent-tree update walk absorbs its recursion-boundary bookkeeping

**What changed.** The recursive range-apply walk now performs its own
bookkeeping at both overlap regimes: total-overlap sites clone the node and
apply the update inline; partial-overlap sites allocate missing children on
demand, flush the pending tag downward (with per-child inline applications of
that tag, including the child-scope conditionals), clone the node for
persistent callers, keep the recursion, and finally recompose the aggregate
from whichever children the recursion produced. The node-cloning unit was
removed from the arena type; the remaining small apply/push/pull units keep
serving the other operations of the type but no longer serve this path.

**Why this site and form.** This is the arena-based, persistence-aware variant
of the same propagation discipline as cluster 3, so the same absorption shows a
different idiom: where the flat tree inlines index arithmetic, the arena tree
inlines allocation, cloning and per-node field work. The recursion itself was
kept, because unrolling it would change the walk's shape beyond what a
plausible tuning session reaches for; the boundary bookkeeping is what got
brought inline here.

**Production role.** This type provides sparse initialization and versioned
history on top of the same range-aggregate semantics; the absorbing walk is
the only code path that maintains arena nodes while updating.

## Cross-cutting design notes

- **Behavior identity.** Every adaptation preserves the exact computed values
  at each step: the rewritten modular leaves agree with the removed reduced
  form at every intermediate value, the reordered scatter passes keep the
  same stable order, and the walk-level conditionals encode the same
  constraints as the removed units' guards. The documented behavior of the
  crate — including panics, empty-range validity, and the persistence
  protocol — is the same before and after the change.
- **Deliberate divergence.** The pastes were intentionally not kept as
  verbatim copies of the removed bodies; site naming, guard shape, arithmetic
  idiom, and comment narration all diverge, while the computation does not.
- **Live-in collaborator survival.** Units whose callers remain — the primality
  test chain used by the public screen, the rational-number gcd idiom in
  `math/num.rs`, the arena apply/push/pull units still used by other tree
  operations, the matcher/trie constructions elsewhere in the crate — were
  left alone; the fold only removed units whose last callers were the
  absorbing entry points.
- **Edge kept honest.** The leftmost-negative example walks and the test
  corpus of the crate were not part of the change; they continue to exercise
  the same entry points as before.
