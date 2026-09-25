# Injection design record — RBTree operators and their absorbed helper layers

## 1. Maintenance motivation

The scenario modeled here is the kind of performance-driven cleanup pass that
a C library goes through once someone profiles it. Red-black tree operations
are small, hot code: every public operation walks the tree once and then does
a bounded amount of surgery. The motivating position for the change is that a
maintainer, unhappy with the call depth on these hot paths — public operator →
fixup helper → primitive helper → accessor — decides the module's operators
should "do the whole job themselves". Manual inlining removes call overhead,
but it also removes what the maintainer considered bureaucratic indirection:
separate rebalancing helpers, separate structure-surgery primitives, separate
parse descent. In reality this pass removes the module's layering: the
operations keep their names and their behavior, but the levels of
responsibility that used to be callable, testable units stop existing as
units anywhere in the file.

This motivation is deliberately chosen for deeply-inlined maintenance work in
C: hot-path optimizations get approved easily, they rarely come with
behavioral changes, and the resulting single-function pipeline is exactly what
a reviewer means when they complain that they "can no longer see what the
operation is doing underneath".

## 2. The normal development evolution being modeled

The change imitates a realistic code evolution witnessed in many data
structure libraries:

1. The module historically had a public operator per operation, each of which
   read as a short orchestration, plus a layer of private helpers
   (node materialization, rebalancing fixup routines, rotation and transplant
   surgery, parse helpers, layout arithmetic) declared in the private header.
2. A maintenance release folds those helper implementations into the
   operators that call them. Callers' bodies grow from a few dozen lines to
   well over a hundred, each containing multiple former helpers' worth of
   mechanics with several levels of nested control flow.
3. The private header stops declaring the absorbed helpers — they no longer
   exist as separable units within the translation unit — and retains only
   the housekeeping helpers nobody folded.
4. Where a primitive was used in more than one operator, it gets
   hand-copied into each site instead of surviving once, because after the
   first inlining there is no shared home left for it.
5. Recursive helpers in the parse and render paths get rewritten as explicit
   stack walks while being absorbed, reflecting the landed commit doing
   inline conversion and recursion flattening at the same time (a common
   pairing for large-input robustness reasons).

The interesting property of this evolution, and the reason it survives in
production codebases, is that nothing observable changes: the demo binary
behaves the same, saved-tree files parse identically, and drawing output is
unchanged. Only the internal organization degrades — which means nothing in
day-to-day use forces anyone to revisit it.

## 3. Overall shape of the change

The change concentrates on the single translation unit implementing the tree
module, `code/RBTree/RBtree.c`, plus its private header
`code/RBTree/RBtree_priv.h`. The public header and the demo program are left
exactly as they are, so the module's programming interface is untouched.

Four operator bodies are constructed, one per major operational pathway of
the module; they share one design idea (the operator now contains the
implementations of everything it calls) but deliberately vary in how that
idea is expressed, which helpers were absorbed, how the absorbed code is
interleaved with the operator's own logic, and what local vocabulary and
commenting style the result uses:

| Region (in module operation order) | Public role | Absorbs the work of | Resulting body |
| --- | --- | --- | --- |
| Insertion operator | insert a key, rebalance on the way out | descent search; node materialization including pool handling; uncle/recolor loop case machinery; the zig-zag rotation case; root recoloring | ~130 lines, 3 levels of nesting |
| Deletion operator | locate and splice out a node, repair the structure | key search; one-child and successor splices; successor slide; four-rung deletion-fix case ladder including three rotation episodes | ~200 lines, 4 levels of nesting |
| Parse operator (tree loading) | rebuild a tree from a preorder text stream | per-record scanning with pool/malloc materialization; the recursive preorder-subtree reconstruction, rewritten over an explicit bound-driven worklist | ~115 lines of scan + worklist machinery |
| Render operator (SVG output) | lay out and draw a tree as SVG | height computation, now an iterative level sweep; recursive subtree drawing, rewritten as an explicit phase-stack walk; position arithmetic at each placement site | ~175 lines, span/phase bookkeeping throughout |

The end state of the module reads the way such code reads in the wild: a
handful of trivial allocation/lifecycle functions and two small recursive
utilities that were not touched, followed by four enormous operator bodies.
Identifiers inside the monoliths change accordingly — each operator promotes
its own local vocabulary (e.g. per-operation names for the scan cursor, the
incoming node, the saved subtree bound), so no two operators appear to share
a naming scheme even where they now contain copies of the same mechanics.

## 4. Cluster rationale

### 4.1 Cluster A — the insertion operator

**What changed.** The insertion entry point keeps its historical shape — find
the insertion position, create the node, wire it in — but now performs every
former sub-step inline. The descent search loop, node materialization (taking
from the module's node pool or falling back to `malloc`), and child/parent
wiring are followed by the whole insertion fixup: a recolor loop with the
uncle-color computation folded into place at each use, the zig-zag
double-rotation case expanded as a labeled section, a final color fixup, and
the root repaint. The former general-purpose rotation helper no longer
exists; the one rotation needed here is expressed directly as pointer
surgery guarded by a single direction flag.

**Why this site and form.** Insertion is the module's most-called hot path,
so it is the most credible first target for a performance-motivated
maintenance pass. It also has a deep call chain (operator → fixup → uncle
lookup → rotation), which after inlining produces the characteristic
appearance this change models: mixed abstraction levels inside a single
body, with the highest-level policy (where the new key goes, when we are
done) interleaved with the lowest-level mechanics (which child pointer gets
rewritten in which direction).

**Production role.** This is the path every later operation depends on for
tree construction; keeping it behaviorally exact while restructuring it was
the deep-risk site of the change.

### 4.2 Cluster B — the deletion operator

**What changed.** Deletion absorbs the most machinery of the four: the
prior node lookup walk, both one-child splices, the successor slide with its
conditional re-parenting, and — largest of all — the four-rung deletion
fixup ladder, each rung expanded from a helper call into inline guarded
sections. Three separate rotation episodes are spelled out inside the
ladder, mirroring what the deleted rotation helper was called for. A
pre-existing oddity of this module's fixup (a sibling recomputation whose two
arms are accidentally identical) is carried over verbatim into the inlined
copy, exactly as a hand-inlined transcript would. One low-level call is
deliberately left in place (releasing the dead node to the module's node
pool), so the resulting function mixes retained calls with absorbed
implementations — the realistic texture of incremental inlining rather than
a uniform transformation.

**Why this site and form.** Deletion is the module's most intricate
algorithm and its deepest original helper chain (lookup → transplant → min →
fixup → rotate), so it is the location where the "operator swallowed the
machinery" sensation is strongest. Its body is also where the copied-primitive
repetition is most visible: the same surgery pattern appearing as several
similar-but-not-identical inline episodes that differ in direction flags
and color handling.

**Production role.** The structural-repair path: any eventual bug in
deletion rebalancing would have to be located inside one ~200-line function
containing search, splicing and four fixup cases at once.

### 4.3 Cluster C — the parse operator (tree loading)

**What changed.** The file-loading operator keeps its current outer skeleton
(open the file, create the tree, produce the root) and absorbs both of its
former helpers: the per-record scanner (semicolon skip, strict `%c, %d`
field parsing, color validation, pool-or-`malloc` materialization) and the
recursive preorder reconstruction. The recursion is rewritten during the
absorption into an explicit worklist of (slot, owner, bound) items, seeded
with the root slot and an unlimited bound; each iteration reads one record
with a duplicated inline copy of the scanner fragment, adopts or rejects the
record depending on the bound, assigns parents (including the module's
observable habit of remembering the most recent owner on the shared sentinel
for empty children), and pushes the children's slots with narrowed bounds.
The worklist grows by doubling.

**Why this site and form.** Parsing is the module's I/O pathway and carries
a semantics-preserving burden the other operators don't: malformed input
handling and EOF mid-stream produce subtly observable states (NULL returns,
partially built trees, sentinel bookkeeping), so it is the natural place to
show an absorbed *iterative* transformation rather than a copied recursive
one. Duplicating the scanner fragment at its two use sites inside the single
operator reflects how inline transcription actually happens when there is
no helper to call twice.

**Production role.** The persistence round-trip path; changes to the file
format or to parse tolerance would today have to be made twice inside one
function.

### 4.4 Cluster D — the render operator (SVG output)

**What changed.** The drawing operator absorbs its height computation (now
an iterative level sweep with two alternating node-count buffers), its
recursive subtree drawer, and the position arithmetic. The recursive draw
becomes an explicit stack of (node, x, y, h, rowpos, phase) items with a
three-phase state machine per node: phase 0 draws the left edge and
re-pushes, phase 1 the right edge, and the final fallthrough draws the
circle and the text label. The seed item's x position and every child
position are computed by an inline copy of the former position-arithmetic
helper. The geometric setup (image width, max-width squeeze factor) stays as
it was; the empty-tree early return before anything is written remains in
place, preserving the long-standing observable rule that drawing an empty
tree creates no file.

**Why this site and form.** Rendering is the module's completely different
execution pathway (linear layout on top of pure traversal), which lets the
same underlying transformation appear with fresh mechanics: level-sweep
buffer swapping and a phased state machine instead of bound-driven worklist
matching. It shares the design idea while sharing almost no structure —
the standard-bearers of "same maintenance decision, different local
reality."

**Production role.** The visualization path; its absorbed recursion plus
layout arithmetic is the classic "one function draws the entire file"
pattern that makes any cosmetic tweak to the output require re-deriving the
whole rendering pipeline.

### 4.5 Cluster E — the private header

**What changed.** The private header's helper declarations shrink to the
housekeeping set that genuinely survives untouched (two teardown helpers and
one traversal helper). Every absorbed helper's prototype is gone, so the
translation unit compiles with the new reality that those routines have no
existence outside the operator bodies. The grouping comments are updated to
describe the module whose helpers were folded together, the way a
maintainer tidies the header after removing the call sites.

**Why this site and form.** The header edit is what commits this to being a
module redesign rather than a local experiment; it is also the most
legible piece of archaeology of the change, because the surviving prototype
list makes the boundary of the absorbed work visible at a glance.

**Production role.** Declares the unit's internal contract; after the
change it no longer names any of the structure-surgery, rebalancing,
parse, or layout machinery.

## 5. Deliberate structural variation across the four bodies

To keep the four sites from feeling like one mechanical template stamped four
times, each embodies a different variant of the inlined style, the same way
they would genuinely disagree had they evolved separately:

- **Mixed granularity.** Deletion keeps one genuine helper call (node
  release into the pool) inside its inlined mass; insertion and render are
  fully absorbed; parse overlaps retained orchestration with duplicated
  scanner fragments. Real inlining passes are inconsistent about which calls
  they keep.
- **Different iteration idioms for absorbed recursion.** Parse absorbs a
  recursive helper into a bound-driven worklist; render absorbs a different
  recursive helper into a phased state machine; neither is a copy of the
  other's mechanism, and both are idioms a C maintainer deploys naturally
  when flattening recursion.
- **Local vocabulary promoted per operator.** The cursor in the descent, the
  slot/bound pair in the worklist, the buffer pair in the level sweep, and
  the phase tags all use operator-idiomatic names rather than the shared
  terms of the former helpers' signatures.
- **Narrative comments rewritten in place.** Where an absorbed stage sits,
  the operator carries comments describing the stage in the vocabulary of
  the operation ("figure out where the new record belongs", "run the ladder
  again"), not the vocabulary of the removed routine — a natural artifact of
  prose being transcribed alongside the code it described.
- **Repeated primitives hand-copied, not parameterized.** Node materialization
  appears as separate inline fragments in the insertion path and in the two
  scanner sites of the parse operator; rotation surgery shows up as one
  guarded episode in insertion and three direction-flagged episodes in the
  deletion ladder; the splice pattern appears three times in deletion; the
  position arithmetic appears three times in render. None of the copies was
  hoisted back behind a flag — the exact fate the original helper design
  existed to avoid.

## 6. Sites deliberately left alone

The change was confined on purpose to the four heavy operators described
above. The module's lifecycle functions and the two small recursive
utilities serving writeout and teardown kept their long-standing structure of
operator-then-helper calls; hollowing those out as well would have meant
absorbing a single shallow call step for no hot-path benefit and raised the
risk to teardown ordering for no gain — and by the time the perf pass was
finished, the flattening imperative had already lost its audience. The
public header, the demo program, and the on-disk formats are untouched.
