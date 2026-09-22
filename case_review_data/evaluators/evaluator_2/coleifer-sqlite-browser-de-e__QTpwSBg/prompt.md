# Maintenance request: complete the removal of unreachable feature scaffolding in sqlite-web

## Where this came from

We audited sqlite-web (the Flask application in this repository) for
half-landed feature work and confirmed that several areas of the package
carry code that cannot execute in the shipped configuration. The shape of
the finding, at the architecture level:

* **Feature branches that cannot activate.** Two of the application's view
  flows — the page that renders a table's structure, and the flow that
  exports a database — contain guarded computations gated behind
  module-level protocol/dialect constants. The values shipped in this build
  keep those branches permanently out of the request path. In one case the
  branch would even fail at runtime if it ran: it reads planner statistics
  that ordinary databases do not maintain until someone runs ANALYZE, and
  our structure page must keep rendering for any database, analyzed or not.
* **Helpers whose only callers are those disabled paths.** The gated
  branches are built on helper definitions in three places: schema
  readers and a small footprint estimate on the dataset class itself,
  formatting helpers beside the views, an encoder in the statement-execution
  module, and a pool-sizing helper in the standalone server script. Some are
  only reachable transitively — a helper used exclusively by another helper
  that itself can never run — and one imported name in the application
  module refers to an executor helper that has no possible active caller,
  so the import list suggests wiring that does not exist.
* **Statements after an early exit.** The multi-statement query runner
  formally contains a few bookkeeping statements written after the
  terminating statement of its error path, where control flow can never
  reach them.
* **A dormant server bootstrap option.** The optional gevent-based server
  script has a pool-sizing path behind a toggle that is disabled in this
  build, so its bootstrap can never select it.

## What we want

Track down every piece of this never-executing scaffolding across the
affected areas — the main application module (its table-structure view and
export flow in particular), the statement-execution module used by the
query screens, the dataset class, and the optional standalone server
script — and remove it, along with everything that exists only to support
it:

* the permanently inactive computation branches inside the views,
* statements left after the error-path early exit in the statement loop,
* helper functions and methods whose only callers were the removed paths,
  or other helpers likewise unreachable,
* import names, module-level constants, and aliases whose only consumers
  were the removed paths.

Removing the scaffolding means the code the repository ships is code that
can actually run. We are not asking you to finish or integrate these
features: the add-on and downstream consumers they were written for are not
part of this task, and turning the dormant paths on is not a fix — the
shipped configuration and defaults must stay exactly as they are.

## What must not change (acceptance boundary)

* Observable request behavior stays identical for the existing application:
  the table-structure page renders the same for any database, including
  ones that have never had statistics collected; content, query, and
  index-screens behave exactly as before; export downloads return the same
  bodies, status codes, and response headers as today.
* The multi-statement query runner keeps its exact current semantics: on a
  failing script it stops at the first error and returns exactly the
  results collected before the failure, with unchanged result contents and
  unchanged statement text in the results it returns for successful runs.
* The standalone server's bootstrap keeps its current fixed pool sizing and
  spawn wiring.
* Utility code that merely happens to sit near the removed scaffolding —
  row-key encoding/decoding used by detail-row URLs, statement splitting,
  single-query execution, the dataset class's caching accessors that live
  templates render — is alive and must remain intact and functional.
* The repository's full existing test suite passes without modification;
  no public API or URL changes.

When you are unsure whether a given helper is part of the scaffolding,
check what executes from the live request and startup paths rather than
what the import list or a comment suggests.
