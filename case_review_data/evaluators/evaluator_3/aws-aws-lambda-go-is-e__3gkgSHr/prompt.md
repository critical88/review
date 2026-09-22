# Restore narrow capability contracts across the runtime integration surfaces

## What we observed

While maintaining the handler pipeline and the runtime integrations we
consolidated their capability contracts onto uniform surfaces, and the seams
that decision created are now the bulk of our friction:

1. The exported handler contract — the interface that user-defined handler
   types implement to plug into this runtime — used to require a single,
   documented invoke capability. Its declaration now lists five behaviors:
   serialized dispatch, streaming dispatch, base-context derivation for
   invocations, a response content-type accessor, and lifecycle release.
   Implementors of adapter types and contributors writing custom handler types
   have to provide all five, and half of them are fabricated obligations: the
   runtime never asks a handler for its content type and never asks it to close
   itself. Response typing (application/json versus raw bytes) and post-send
   release of returned responses have always been decided where *returned
   response objects* are consumed — the content-type and closer probes in the
   response path, plus the pooled-buffer recycling — not by handler-level
   capabilities.
2. Handler construction now accepts user types through a private single-capability
   alias instead of the exported contract, because types exposing only the
   documented serialized invoke capability no longer satisfy the contract
   directly. That is a telling marker of how far the contract has drifted from
   what the platform consumes.
3. Both execution modes — the provided-runtime API loop and the deprecated
   go1.x net/rpc function mode — are now typed against the widened handler
   contract rather than the handler-options holder they used to work with. Each
   mode's dispatch body uses only one or two capabilities of that list, so the
   widest capability surface in the package is now the dependency of every
   invocation path.
4. The HTTP client capability surfaces used by the runtime API client, the
   extensions API client, and the custom-resource response sender describe the
   full convenience request surface of a standard library client, including the
   generic key/value request helpers, although every one of those integrations
   builds its requests explicitly (per-call endpoint paths, signed runtime
   requests, per-call headers, extension registration bodies) and none uses the
   convenience verbs. The same widening applies to the response sender used by
   the CloudFormation custom-resource flow, which issues one explicit PUT and
   removes the Content-Type header before sending.
5. The package's own test doubles grew capability members to keep compiling
   against the widened contracts — several of which nothing in their tests
   exercises — and the entry-point documentation advertises the full capability
   list as what an implementor must provide.

## The design concern

Responsibility mismatch between capability *declaration* (what contracts demand
from implementors) and capability *consumption* (what callers of those contracts
actually dispatch). The principle we want back in force in this area: an
implementor, or a consumer bound to a contract, should never be forced to depend
on behaviors that the contract's users do not use. Capabilities that are
genuinely dispatched through a contract — streaming dispatch into the response
pipeline, the configured invocation parent context, prepared-request execution
over HTTP — belong on contracts. Capabilities that nothing dispatches — the
handler-level content-type or release capabilities, the transport-level
convenience verbs — are pure obligation: adapters and user types grow methods
that exist only to satisfy declarations.

## Scope

The request-invocation subsystem of this repository — handler construction and
the adaptation of function values, the runtime API invoke loop, and the
deprecated net/rpc function mode — together with the HTTP client capability
surfaces used by the runtime API client, the extensions API client, and the
CloudFormation custom-resource response delivery. The same underlying problem
appears in several places across these areas; this is one cleanup task, and it
is complete only when every instance is addressed.

## What the finished state should look like

- Every capability contract in the affected area demands only capabilities that
  the repository's runtime paths genuinely dispatch through that contract, and
  contract consumers depend only on what they use.
- Adapter types inside the pipeline no longer carry fabricated capability
  members, and nothing anywhere provides a capability for declaration-satisfaction
  alone.
- Custom handler types exposing the documented serialized invoke capability
  satisfy the exported handler contract again, and every start path that
  historically took them accepts them again directly; no internal back-compat
  seam is needed to route values around that contract.
- Where a capability accessor exists so a contract could be read through, the
  consumers read the composition state they need directly instead.
- The HTTP client capability surfaces require only the prepared-request
  execution capability their consumers actually use.
- The documentation entry points that describe what an implementor must provide
  again match the restored contracts.
- Adjustments to test files are limited to removing or trimming members that
  existed only to satisfy a widened contract; no assertion may be weakened and
  no exercised behavior of the test suite may be dropped.

## Behavior and compatibility that must not change

- Both execution modes keep their current invocation behavior end to end:
  per-invocation deadlines derived from invoke metadata, the configured base
  context as invocation parent in both modes, dispatch through the
  function-value adaptation pipeline, streaming (io.Reader) responses, the
  response typing and release that run off the returned response objects, and
  the current failure and error reporting.
- All exported start paths and options keep their names, roles, and accepted
  inputs: plain function values, user-defined handler types exposing the
  documented invoke capability, and the existing option functions behave
  identically for every caller that was valid before the capability surfaces
  were consolidated.
- The extensions registration and SIGTERM wiring, the response buffer pool
  round-trip, and the custom-resource response delivery behavior remain as they
  are.
- The package's full test suite continues to pass.
