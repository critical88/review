# Injection design record — shomali11-go-interview-dc-n

## Motivation

This repository is a collection of small Go interview-style algorithm and
utility packages (a generic A* path-finding engine with a grid-world
implementation and a PNG image writer, a memoized word breaker, streaming
statistics helpers, and reusable container libraries). The maintenance work
modeled here starts from a plausible backlog item for such a codebase:

> "Split the algorithm packages into a reusable library plus a thin demo/CLI
> layer, so the grid solver, the word utility, and the window statistics can
> be driven from places other than their current structs."

To prepare for that split, one engineer ran a "make the internals explicit"
sweep before the API freeze. The reasoning at the time is the reasoning that
shows up in real reviews: internal computation steps that reach into their
owning struct are hard to call from a second entry point, so the sweep
extracted those steps into package-level helpers that receive what they need
as parameters. The structs stayed exactly as they were — public surface
frozen — while internal steps became freely callable.

The sweep was useful and it stopped at "parameters are explicit". It never did
the second half of the job — deciding what those parameters *mean* — because
the library split was re-prioritized after the first working iteration. So the
working state of each abstraction now travels through the code as loose
individual parameters, and each helper that was extracted grew the same
parameter sequence as its siblings.

## Normal development evolution being modeled

This is the ordinary shape of an unfinished but shippable modernization pass,
of the kind every long-lived codebase accumulates:

- extraction for reuse ("I need to call the eviction math from the batch
  runner"), not extraction for design;
- the extracted helper receives as parameters exactly what the original
  method body read off the owning struct, which is why the same members keep
  appearing side by side in every signature;
- some helpers were converted and similar ones in the same file were left for
  a follow-up pass, which is how lookup steps ended up outside the owning
  grid world while the basic accessors stayed methods;
- one helper was added late (a dictionary/memo probe factored out of the
  recursion) and copied the parameter sequence of its callers because that
  was the established local convention by then.

No behavior was meant to change during the sweep: every extracted helper
performs the same computation on the same values, and the public constructors,
methods, and demo entry points kept their names and signatures so the
package's consumers would not notice the internal churn.

## Overall design

Four independent maintenance items of the same kind were applied, one per
package area, each converting previously encapsulated per-owner state into
explicitly threaded parameters inside that area:

| Area | Files | State that became parameters |
| --- | --- | --- |
| A* engine internals | `algorithms/astar/a_star.go` | per-search bookkeeping: open priority queue, closed set, node-to-wrapper registry |
| Grid navigation + image export | `algorithms/astar/grids/grid_world.go`, `grid_node.go`, `grid_generator.go` | grid world plus cell coordinates; obstacle-shadow lookup helper |
| Word breaker | `strings/wordbreakers/word_breaker.go` | parse state: input, dictionary set, memo cache |
| Moving averages | `streams/movingaverages/moving_averages.go` | sliding-window bookkeeping: running sum, capacity, element queue |

The four areas are functionally unrelated (search, rendering, parsing,
statistics), so the sweep's residue is visible in four packages at once; the
injected diff deliberately keeps each area's changes self-contained so the
code reads as four separate working-session leftovers rather than one
engineered pattern.

## Per-cluster record

### Cluster 1 — A* search loop (`algorithms/astar/a_star.go`)

**What changed.** `AStar.Search` no longer contains the traversal; it
delegates to a package-level `search` function. The relaxation step and the
between-searches reset were likewise extracted as `updateVertex` and
`clearSearch`. All three helpers receive the engine's working structures —
the open priority queue, the closed set, and the node-to-wrapper registry —
as individual parameters. `getWrapper`, which only needs the registry
alongside one node, became a free function taking just those two.

**Why this shape.** The traversal loop, the vertex relaxation, and the reset
are the three pieces a second caller (say, a batch re-solve step or an
instrumented variant) would want to drive separately, so those are what the
sweep pulled out. Because each of them operates on all three bookkeeping
structures, all three appear in each new signature; the two genuine per-call
inputs (the start and goal nodes for the traversal, the current/neighbor/goal
wrappers for the relaxation) sit in front of the working structures.

**Production role.** This is the hot path of the pathfinding demo: engine
setup in `New`, one `Search` call per query, and a reset so the same instance
can be reused across queries. The reset helper mutates the registry in place
(ranging over the map and deleting entries) so the structures the instance
holds really are cleared between queries — the previous inline code cleared
the very fields on the struct, and the extraction must not change that.

### Cluster 2 — grid navigation and painting (`algorithms/astar/grids`)

**What changed.** Three small additions in `grid_world.go`: `isOpen`
(bounds-plus-obstacle check for a position) and `nodeAt` (bounds-guarded
lookup of a position) as free functions taking the world and two coordinates.
`GridNode.GetNeighbors` no longer reads its own `world` back-reference for
those steps; it computes the neighbor coordinates and calls the free helpers
with the world alongside them. The obstacle-shadow test that used to run
inline in `GetNeighbors` was extracted as `isBehindObstacles` — a free
function that again takes the world and the moving node's coordinates plus
the neighbor being examined, and re-checks the four diagonal corner cases
exactly as before. Finally the PNG writer grew `drawCell` in
`grid_generator.go`: the per-cell paint step of the image export, taking the
world, a row and a column, and the drawing handle and font already in use,
with a new `font.Face` import for its signature. `createImage` kept its
signature because the demo test calls it directly after a solve.

**Why this shape.** The grid package's whole job is asking "what is at
(i, j)?" — the sweep's author made that question explicit by passing the
world and the position to every helper that asks it, whether for traversal
(open? node?) , for obstacle-shadow logic (both corner cells?), or for
rendering (paint this cell). The world-plus-two-integers sequence therefore
recurs naturally across the extraction sites; the coordinates themselves are
computed at each call site from the moving node, matching what the old inline
code did with loop counters.

**Production role.** Neighbor generation (with the obstacle-corner filtering
that makes diagonal moves realistic) is what the A* engine exercises on every
expansion, and the per-cell paint loop is what produces the deterministic
`output.png` the grid test regenerates. The node accessor pair also backs the
custom multi-robot example, so the demo path and the test path both run
through the extracted helpers.

### Cluster 3 — word breaker parsing state (`strings/wordbreakers`)

**What changed.** `BreakWord` now builds its dictionary and delegates to a
package-level `breakWord` that takes the input plus the dictionary set and the
memo cache as parameters. A late extraction, `firstKnownWord`, factored out
the probe that answers "is the whole remaining input a dictionary word, or do
we already have a cached result for it?" — it takes the same three values,
using them as the established local convention. `ExtractWords`'s recursive
worker became a free `extractWords` carrying the identical trio.

**Why this shape.** Word breaking is recursive with one memo per top-level
call; once the first helper was extracted for reuse ("call the recursion from
the batch vocabulary expander"), the memo and dictionary had to travel with
it. The memo is keyed by the very input that also rides along as a parameter,
and the input genuinely changes at every recursion level — the sweep did not
stop to ask which of the three values is shared parse state and which is the
value being parsed; it threaded all three uniformly.

**Production role.** `BreakWord`/`ExtractWords` answer the demo questions
("can this sentence be split into dictionary words? which words occur?").
The dictionary lookup with memoization is the core of the package; the cache
is created per public call and discarded with it, so results never leak
between calls.

### Cluster 4 — moving-average window math (`streams/movingaverages`)

**What changed.** `MovingAverage.Add` and `GetAverage` now delegate to
package-level helpers: `addToWindow` (append a number, then evict), `evict`
(drop the oldest value when the queue outgrew the capacity window), and
`windowAverage` (divide the tracked sum by the current queue size). Each one
receives the window's bookkeeping — the running sum, the window capacity, and
the element queue — as separate parameters; the delegation from `Add` passes
the owner's `sum` field by address so the helper mutates the same value the
`GetSum` accessor reads. `GetSum` did not need extraction and stayed a one-line
accessor.

**Why this shape.** The window append/evict/read trio is the piece the sweep
wanted callable from an ingest pipeline, so it followed the same recipe as
everywhere else: take what the method body used — field reads
`ma.sum`/`ma.windowSize`/`ma.queue` — and turn each into a parameter. All
three helpers ended up with the uniform three-value signature even though the
average read does not consult the capacity: uniform-by-convention, like the
rest of the sweep's residue.

**Production role.** This is the sliding-window statistic behind the
`GetAverage`/`GetSum` results, including the precise eviction order (add,
enqueue, then evict the oldest past capacity) that downstream comparisons
rely on.

## Deliberate structural variation

The four clusters were left intentionally non-uniform, in the way real
maintainer slop is non-uniform:

- **run position varies.** In the A* engine the recurring parameter run sits
  in the middle of `updateVertex` (behind three genuine per-call wrappers)
  and at the tail of `search` (only the two nodes come first); in the grids
  helpers the run starts the signature; in the word breaker and the window
  math it also leads, but with a genuine per-call value (`input`; `number`)
  after it in two of the helpers.
- **partial conversion.** `extractWords` in the word breaker and `getWrapper`
  in the engine kept/adopted shapes that do not carry the full group, as
  organic leftovers; the window's `windowAverage` reads the capacity it never
  uses; the grid world's basic accessors (`Get`, `IsValid`) kept their
  receiver form while their new free siblings sit two paragraphs below.
- **derived members.** The grid helpers' coordinates are recomputed at each
  call site from the moving node and loop counters (they are not pre-stored
  anywhere), and the obstacle-shadow helper takes the moving node's own
  coordinates plus the neighbor, so the members of the world-plus-position
  run come from different sources.
- **aliasing and identity.** The window helpers take the owner's sum by
  pointer (`*int`), the engine helpers pass reference-typed structures, and
  the word breaker's memo is one map instance per public call threaded
  through recursion — three different ways a recurring member can be tied to
  its owner, all in one diff.
- **one cluster spans a demo-critical artifact.** The paint helper keeps the
  PNG writer cell-for-cell identical to the previous inline loop ("Block"
  for obstacles, cost text otherwise), which is where the sweep had to be
  most careful.

## Public surface kept stable

The sweep explicitly froze the public surface: `astar.New` and
`AStar.Search`, the grid constructors and accessors including the
`createImage` helper the demo test invokes directly, the PNG output the grid
test regenerates, `wordbreakers.BreakWord`/`ExtractWords`, and
`movingaverages.New`/`Add`/`GetAverage`/`GetSum` all keep their names,
signatures, and observable results. The changes live one level down, in the
internal helpers and their call sites.
