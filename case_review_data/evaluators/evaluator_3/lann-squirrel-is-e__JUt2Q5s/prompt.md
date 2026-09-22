# Refactor request: capability contracts in the SQL builder's runner plumbing

## Context

squirrel renders SQL with fluent builders and hands finished statements to an
execution target — usually a `*sql.DB`-like value — through a family of small
capability interfaces. Historically each seam named exactly the method family
its call path used: Exec-style execution, Query-style execution, query-row,
the context counterparts of each, and a prepare seam that the statement cache
consumes.

Over the past release the maintainers consolidated this "interface zoo". One
unified runner surface now fuses the synchronous execution family, its
context counterparts, and the prepare capability, and the existing seams
were re-pointed at it. The old small-interface names survive as aliases or
re-declarations of the new surface.

## The observation

I am wiring up a storage adapter that implements only a subset of the
execution repertoire — it can execute, and execute with a context, and
nothing else. Before the consolidation it plugged straight into the
execution-only seams. Today it no longer fits them, and reading the package
shows why: several seam declarations now demand capability that nothing
behind them ever calls on the value they receive.

- The one-shot execution helpers each still invoke exactly one method on
  their target parameter, but both are typed against a shared synchronous
  runner shape that also carries the sibling capability the helper ignores.
- All three context execution seams are now the same fused
  synchronous-plus-context surface, although each helper invokes exactly
  one of those methods.
- The statement cache and every one of its constructors only ever call the
  prepare pair on the values they receive, yet their declared contracts
  force the whole execution surface onto them; the proxy constructor now
  also hard-codes one particular wrapper to keep the cache working against
  a standard database handle, instead of accepting the handle for what the
  cache actually uses.
- The public placeholder-format contract grew a debug hook, so every
  statement-data renderer that stores a format now carries a debug
  capability that normal statement rendering never invokes.

The runtime plumbing still narrows values by capability at execution time
(private probes yielding the documented dedicated errors), so the static
declarations and the real dependency of these seams are now out of sync —
that mismatch is what I want fixed.

## Requested outcome

Re-split the over-consolidated runner plumbing so that each dependency
point — parameters and stored fields in these seams — requires only the
capabilities its own call path actually exercises on the supplied value:

- Restore focused, cohesive capability declarations for the execution
  seams, choosing whatever names and structure you judge best.
- Give the statement cache prepare-scoped contracts for its field and
  its constructors, and reconcile the standard-database path
  (wrapper, proxy constructor) so a plain standard-library target keeps
  connecting without carrying the merged surface.
- Restore the placeholder-format seam to the replacement-only capability
  that statement rendering consumes; the debug path may keep discovering
  its token capability however you see fit.
- Keep compatibility aliases only where the package genuinely still earns
  them, and do not re-impose the merged obligation anywhere else to make
  things compile.

Equivalent redesigns are acceptable. I care about each seam's dependency
boundary matching what the seam actually consumes, not about any specific
interface names or shapes.

## Boundaries

- **Behavior.** Every builder keeps rendering byte-identical SQL and
  argument slices for all inputs, including nested expressions, aliases,
  case expressions, and composed where/limit/offset forms. Debug
  rendering keeps producing the same strings.
- **Error semantics.** The error identities stay: an execution attempt
  with no runner configured still reports `RunnerNotSet`; the query-row
  path still reports `RunnerNotQueryRunner` when the configured runner
  lacks the query-row capability; the context methods still report
  `NoContextSupport` when a probed target lacks the context capability.
- **API continuity.** No exported identifier disappears from the package:
  the builder families, the statement-cache constructors, the placeholder
  implementations and variables, `DebugSqlizer`, the wrapper helpers, and
  the capability interface names downstream code references (the Exec,
  Query, query-row, context, prepare, and group names) remain exported,
  with whatever signatures the re-split requires.
- **Scope.** Treat the whole package as in-bounds for the seam work, but
  do not restructure SQL rendering, builder compositing, or the public
  fluent API beyond what the seam re-split requires. Keep existing tests
  passing; mechanical adjustments caused by signature evolution are fine,
  but do not re-point tests at a redesigned surface merely to make them
  compile.
