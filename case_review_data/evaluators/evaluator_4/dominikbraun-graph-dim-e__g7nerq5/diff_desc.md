# Injection design record: performance pass that dissolved the helper layer

<a name="motivation"></a>
## Maintenance motivation

The Go graph library keeps a small internal layer of collection helpers: a
generic `stack` with a membership registry, a `priorityQueue` over a binary
heap, a `unionFind` for disjoint sets, and a `stackOfStacks` used to explore
layered branches. The public algorithms - walks, searches, component
detection, spanning trees - were all written to consume those helpers.

The change modeled here is the most common way such layering dies in a
performance-sensitive library: a contributor optimizes the hot paths by
"removing the indirection" of tiny helper methods, copies each helper's body
into its only or most frequent caller, and then removes the helpers that ran
out of callers as a dead-code cleanup. Each step looks defensible in
isolation - the call graph really does get flatter, the helper really is
unused afterwards - and the repo's habits (small commits, conventional doc
comments) are respected throughout. The result is an
algorithm layer that has absorbed its own substrate.

Both surviving tendencies of this library support the story: the CHANGELOG
records routine performance work, and the in-memory store already carries a
hand-specialized `CreatesCycle` shortcut next to the general algorithm, so
copy-for-speed is an established move in this code base.

## Normal development evolution being modeled

The diff reads as a short series of ordinary commits on the pinned revision:

1. **Frontier walks stop calling the stack.** The three consumers of the
   generic `stack` replace `push`/`pop` with inline slice bookkeeping plus a
   local membership map, with comments motivated by the per-call overhead.
2. **The shortest-path search stops calling the priority queue.** The
   relaxation loop takes over the heap directly.
3. **The SCC walk loses its recursion.** The recursive helper holding the
   Tarjan state is converted into an explicit machine inside the public
   function, which is the conventional way to silence recursion-depth
   complaints on large graphs.
4. **The spanning-tree builder goes fully self-contained.** All of its helper
   delegations - constructor chain, property-copy option builders, union-find
   - are copied in.
5. **Dead-code cleanup.** With no production callers left, `stackOfStacks`
   (four methods plus constructor) and the SCC pair `findSCC`/`sccState` are
   deleted, the way an unused-symbol sweep after such a pass normally would.

Commit-sized, each one shippable, each one justifiable; the layering only
disappears in the sum.

## Overall design

The injection applies one coherent editing discipline - "the algorithm body
owns the machinery it needs" - while varying the *shape* of the absorbed code
from site to site, so the result looks like accumulated optimization work
rather than a repeated stencil. Design rules used throughout:

- **Copy the helper's instructions, not a summary.** Each inlined fragment is
  a line-level translation of the helper it replaces, so behavior - including
  established quirks - carries over unchanged. The union-find compression,
  which rewrites the *entry* vertex on every step of its compression walk,
  is reproduced slot-for-slot inside the spanning-tree builder; fixing it
  while copying it would have changed behavior.
- **Keep what has its own audience.** Helpers with direct unit test coverage
  (`stack`, `priorityQueue`, `unionFind`) and helpers with remaining
  production callers (`copyVertexProperties`, `copyEdge`, `NewLike`) are left
  in place; only the helpers left with no production audience disappear.
- **The documentation follows the code a contributor would really leave
  behind.** Doc comments that described delegation ("non-recursive and
  maintains a stack instead") are reworded to describe the new
  self-contained mechanics in the library's own voice, so the diff contains no
  comment claiming something the code no longer does.

## Sites and clusters

### a) Three frontier walks (`CreatesCycle` in paths.go, `DFS` in
### traversal.go, `TransitiveReduction` in dag.go)

**What changed.** Each of these functions previously kept its open vertices in
the shared `stack`. All three now carry the stack's implementation privately:
an open slice consumed from its end, a membership map that is inserted into on
push and deleted from on pop, and the same visit-once guarding.

**Why these sites.** They are the stack's only algorithm-layer customers, and
they sit on the three hottest read paths of the library (cycle checks run
inside `AddEdge` guards, DFS is the public traversal entry point, transitive
reduction runs a nested walk per edge).

**Why different shapes.** The three inlines deliberately do not mirror each
other: the cycle check walks predecessors with an early target match, DFS
stops through the user's visit callback, and the reduction keeps per-root
traversal state with cycle diagnostics - three different nesting profiles and
error obligations around the same core mechanics. Local concurrency
(`queued`/`onPending` names, comment voice) matches each file's habits.

**Production role.** These functions are the library's graph-walking
workhorses; after the change, each also privately owns a stack
implementation, three times over, next to the still-tested generic one.

### b) The SCC recursion (`StronglyConnectedComponents` in paths.go)

**What changed.** The function previously delegated to a recursive helper that
carried a `sccState` record through the Tarjan walk. That helper and its
state type are gone; the function now drives the walk with explicit per-root
stacks (frames, per-frame cursors, successor lists) and an open trail with a
membership set, performing the lowlink bookkeeping, the retreat logic, and
the component drain inline. The conversion follows the iterative-Tarjan idiom
a Go contributor would write, and the largest cluster of comments in the diff
explains it in review-oriented prose.

**Why this site.** Recursion-to-iteration is the classic follow-on to
"helpers are overhead" work: the helper *is* the recursion, so removing the
helper means removing the recursion. The component stack semantics
(lowlink checks, back-edge handling, drain-until-pivot) moved in wholesale.

**Production role.** SCC detection is the most algorithmically dense public
function in the package; after the change its entry function also
contains the full control-state machinery of the algorithm it serves.

### c) The layered exploration (`AllPathsBetween` in paths.go)

**What changed.** The function previously ran its branch bookkeeping through
`stackOfStacks` and a small build-layer closure. Both are absorbed: postponed
successor sets live in a local slice-of-slices, each new layer is built in
place, and the two `unable to remove/build layer: empty stack` guard errors -
the removed helper's own defenses - are now raised from the algorithm body
even though no stack type is involved anymore.

**Why this site.** It was `stackOfStacks`'s only customer, so after the
inlining the helper had no production caller at all; its guard errors are the
change's most visible residue and the clearest sign that the deletion swept
out a helper whose contract still mattered.

**Production role.** Exhaustive path enumeration; also the one place where the
diff keeps a fully defensive guard that today can no longer be exercised
through any public entry point - exactly the kind of archeology that
maintenance work later trips over.

### d) The relaxation agenda (`ShortestPath` in paths.go)

**What changed.** The function previously drove the relaxation through the
`priorityQueue` helper. It now constructs `priorityItem` values itself, calls
`heap.Push`, `heap.Pop`, and `heap.Fix` on a local `minHeap` directly, and
maintains the helper's cache semantics itself: insert on enqueue, delete on
dequeue, cache lookup-and-rebalance on relaxation. The reconstruction comment
notes that the agenda keeps "mirrors of the queue bookkeeping previously done
by a dedicated helper type" - the pass that modeled this change left that
much honesty in the comments.

**Why this site.** It is the only consumer of the priority queue; a
single-caller helper is the first thing "unindirection" work absorbs. The
path-reconstruction half of the function was left untouched, showing the
usual selectivity of such a pass: only the loop touching the helper changed.

**Production role.** Dijkstra-style weighted shortest path, the library's
heaviest read path after traversals; after the change it also embeds a
priority queue implementation beside the tested generic one.

### e) The self-contained spanning-tree builder (`spanningTree` in trees.go)

**What changed.** This site absorbs the widest stack of helpers, three levels
deep: the `NewLike`→`New`→`NewWithStore` constructor chain becomes a literal
`undirected` struct with hand-copied trait fields and a raw in-memory store;
`copyVertexProperties` becomes an inline attribute-by-attribute vertex copy
written straight into the store; `copyEdge` becomes a literal `AddEdge`
option closure; and `unionFind` (`add`, `find` with its compression walk, and
`union` with its root-to-root join) becomes a local parent map with inline
marches, inline compression, and an inline join. The comment set frames this
as "assembled right here instead of going through the graph constructors".

**Why this site.** `spanningTree` is the deepest consumer chain left after
steps (a)-(d): every one of its helpers had effectively one customer, and a
contributor who has already absorbed simpler helpers reaches for the same
move here. The union-find compression - quirky but observable - is carried
over literally; the store allocation the constructor chain used to hide is
now spelled out, including the fresh `VertexProperties` map a caller used to
get from the option builder.

**Production role.** The single implementation behind both public MST
functions; after the change it reads less like tree-building logic and more
like an unrolled constructor plus a disjoint-set type plus an option builder
plus tree-building logic.

### f) The deletions (`collection.go`, `paths.go`)

**What changed.** `stackOfStacks` with its four methods and constructor, and
`findSCC` with `sccState`, are removed after losing their last production
callers. The still-tested `stack`, `priorityQueue`, and `unionFind` types, and
the still-called builder helpers, remain untouched.

**Why only these.** An unused-symbol sweep after such a pass removes code
with no remaining audience; deleting more would have meant deleting tests,
which even a hurried pass usually notices. The asymmetry - helpers die only
when their last algorithm stops calling them - is what makes the deletions
look like a consequence rather than a goal.

## Deliberate structural variation

The case was built so that no two absorbed sites are shape-clones: recursion
flattened into machine state (b) beside simple loop-local bookkeeping (a);
an interface-typed option builder replaced once by a literal closure (e) and
left alone in its other callers; heap mechanics with a side-table (d) beside
pure slice/registry pairs (a); a store-level mutation replacing `AddVertex`
delegation (e only); guard errors migrating with their machinery (c) while
straight error paths stay wrapped exactly as before (a); and per-root versus
per-function state scopes giving the frontier walks different nesting
pressures. Comments match each file's voice, and only the sentences a real
contributor would have rewritten were rewritten.

The change set stays inside the root package's production files; the test
suite, the `draw` subpackage, and all exported signatures are outside its
scope by design, because a performance pass of this kind would have no
reason to touch them.
