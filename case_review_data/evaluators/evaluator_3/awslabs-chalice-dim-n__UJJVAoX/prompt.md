# Maintainability regression: a few of our long entry points have grown into single giant bodies

We've been paying a growing maintenance tax in two parts of the codebase, and it
is time to pay it down properly.

Last sprint two of us spent an afternoon tracing one HTTP request through the
local testing server (`chalice local`). The request path and the authorizer
verification that runs alongside it have each grown into one long top-to-bottom
routine; the same body mixes together unrelated jobs like looking up the
registered route, interpreting the request credentials, synthesizing the
Lambda-shaped event, answering the OPTIONS/CORS preflight, and checking the
returned policy. Reviewing or hot-fixing any one of those jobs means reading
all of them, and a fix has to be made at one specific indentation somewhere in
the middle of the wall.

The deploy side has grown the same way. The routine in the AWS client that
updates a Lambda function during a redeploy now carries the whole sequence --
code update with its retry/error handling, waiting for the function state,
assembling the configuration payload, reconciling tags -- as one body. The
template generator for websocket APIs assembles every template section
(integrations, routes, the stage, the optional custom domain, the output
section) inline. And the redeploy sweeper that decides what to clean up runs
its marking pass, its per-resource diffing, and its deletion planning as a
single giant routine. A month ago we fixed a tag-handling bug in the wrong
place first, because near-duplicate logic for the same stage appears at
several different indentation depths inside one of these bodies.

The rest of the codebase does not read this way: there, the same flows are
expressed as short, well-named methods where each step of a pipeline is its
own scoping unit, and entry points read as a short sequence of named steps.
These five-or-six hot spots read like several different programs pasted
together.

Please investigate the affected areas -- the local-mode request-serving and
authorizer machinery, the AWS client's function-update path, SAM websocket
template generation, and the redeploy resource sweeper -- and restore the
usual structure, consistently, wherever you find this pattern in those areas.
This means appropriately scoped methods at sensible abstraction levels so each
pipeline step is a named, reviewable unit again, not just the most obvious
spot. Do not do a cosmetic fix (renames, comment rearrangement, or dumping a
chunk of the body behind one arbitrary helper do not help readability, and
one of us tried that on a branch and it made review worse).

Hard requirements:

- Behavior must not change in any way. The complete test suite
  (`python -m pytest tests/unit tests/functional tests/integration`) must pass
  without modifying any test. Do not edit or delete tests to make this work.
- Keep public behavior and artifacts identical: the local gateway and
  authorizer entry points keep their signatures and error responses, the
  generated SAM/CloudFormation templates stay identical for equivalent apps,
  and the deploy plan ordering semantics (deletion order, and where api
  mapping deletions are placed) are preserved.
- The cleanup must be complete across the affected areas: wherever the same
  "everything inline" pattern occurs in them, it gets the same treatment.
