# Retire the dormant pre-modern compatibility paths in aiopg

## Context

aiopg consolidated several behaviors in its 1.x line: asyncio event-loop
handling became running-loop-only, LISTEN/NOTIFY delivery moved onto the
connection's queue, bulk protocols were rejected outright in the async-only
API, and the SQLAlchemy integration layer followed SQLAlchemy 1.4
conventions. Each of those transitions kept the previous behavior reachable
for deployments that had pinned the older API, behind module-level
switches that ship permanently disabled. Those release-side decisions are
now final: the older behaviors are no longer documented, no supported
configuration enables them, and the two minor versions past the
consolidation have shipped with them switched off.

The retirements were only partially carried into the code. The disabled
branches are still wired into live methods, and the definitions that exist
solely to serve them are still present. In a few places the old
implementation was even left physically inside a method after the method had
already been changed to fail unconditionally, so it sits after an
unconditional error and can never be reached. Nothing in supported usage
executes any of this.

## Affected components

Two layers carry the remnants, and both need the same treatment:

- **The core driver layer** — the connection/pool objects, their
  constructors and lifecycle helpers, the event-loop plumbing module they
  share, and the notification delivery loop. Remnants here include the
  dormant option to adopt a caller-supplied event loop, a registration
  interface for channeling notifications into integration-owned containers,
  an audit option in the pool's idle-connection recycling decision, and
  superseded code left behind the async-only guards for query cancellation
  and the bulk copy protocol.
- **The SQLAlchemy integration layer** — the module-level execution/parameter
  handling, the psycopg2 dialect compiler, the Mapping-style result-row
  type, and the transaction hierarchy. Remnants here include a dormant
  raw-positional-parameter execution style, the older second-pass way of
  resolving server-side defaults during statement compilation, an older
  raw-positional row access style, and a pre-modern ancestor walk left
  inside nested transaction commit after it became a hard error.

## Task

Remove these dormant compatibility paths completely:

1. Delete the disabled branches and the unreachable code left behind
   unconditional failures, so every participating method contains only
   statements that supported inputs can execute.
2. Delete the module-level switches whose only purpose was to enable those
   branches, together with every private helper, constant, registry, and
   import name that only served them. After the removal, no definition may
   remain whose only reason to exist was one of the retired paths —
   including names that look like ordinary imports at the top of their
   modules.
3. Keep the library's supported behavior exactly as it is. Public API
   surface, function signatures, and the names exported by the package
   must not change; the removed names are all private module-level
   entities that nothing outside the package uses. Runtime behavior under
   supported configurations must be identical: event-loop policy,
   notification delivery, the async-only error guarantees, pool recycling,
  compiled statement parameters, row-access semantics, and transaction
   error semantics all stay as they are today.
4. Pursue the full set of leftovers in both layers, not only the most
   visible ones. The manifestations differ in shape — guarded branches,
   code left after a failure, orphaned helpers and constants — and some
   exist as isolated names that only a package-wide look can confidently
   retire. Removing one instance must not leave its supporting definitions
   behind elsewhere.

Do not restructure anything beyond what the removal requires: no renames
of public names, no changes to error types or messages of live code, no
test changes, and no new behavior.

## Completion criteria

- Every retired-path branch, unreachable remnant, switch, helper,
  registry, alias, and name that served them is gone from both layers.
- All statements in the touched modules are reachable under supported
  configurations.
- The full test suite passes with the same results as before the change.
