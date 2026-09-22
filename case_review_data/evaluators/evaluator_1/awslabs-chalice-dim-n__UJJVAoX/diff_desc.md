# Injection design record — awslabs/chalice, `deeply_inlined_method` case

This document records the design of the staged change set: the maintenance
story it models, the overall shape, and a per-location rationale for every
materially changed cluster. It is an auditable account of the choices made
while editing, not a review or a verdict, and it does not predict what any
particular reader should conclude from the diff.

## 1. Realistic maintenance motivation

Chalice's pipelines (local-mode request serving, the AWS deploy client, SAM
template generation, redeploy cleanup) are normally written as short,
well-named methods composed one to three levels deep under an entry point.
That is the house style, and it is what makes the framework navigable.

The evolution modeled here is the one that erodes house style in real projects:
several focused bug fixes land in a hurry, each inside one pipeline. While
debugging a streaming or staging bug, the developer steps *into* the helper
two frames down, edits the logic, and then keeps both frames of context in
mind; after the second such fix it is tempting to stop jumping and simply
keep the helper's statements up in the caller "just for this oncall week",
with a comment marking the region. Over successive hotfixes this happens at
more than one call depth, the callee disappears (its remaining callers count
drops to one, then zero, then it is deleted), and the entry point is left
holding the entire former call chain verbatim. Nobody plans this; review load
per hotfix is tiny, and the aggregate result only shows up quarters later.

We modeled the terminal state of that process at five entry points across
four files, chosen because each owns a distinct, behavior-rich pipeline of
its own.

## 2. Overall design of the change set

Each cluster below is one entry method whose helper chain (three or more
levels deep in at least one branch of the original call graph) was collapsed
into the method body, with the now-unreferenced helpers deleted:

- `#local.py` — the local authorizer verification entry and the local
  request-serving entry (two clusters in one file).
- `#awsclient.py` — the typed AWS client's Lambda function-update entry.
- `#package.py` — the SAM template generator's websocket API generator.
- `#sweeper.py` — the redeploy resource sweeper's single entry.

Deliberate structural variation across clusters, so that no single textual
or syntactic recipe describes them all:

- Some regions keep the original variable names of their absorbed helpers
  intact; others rename (`name` → `deployed_name`/`resource_name`, `rem` →
  kept, `properties` → `domain_properties`) or restructure around different
  local grammars.
- Idiom rotation: a `getattr`-based dynamic dispatch is replaced by an
  if/elif ladder in one cluster while a different dispatch in another
  cluster is deliberately *kept* as `getattr`; a dict-lookup-with-default
  becomes an if/elif/else chain; repeated per-item blocks become a loop over
  a name tuple; one comparison is a conditional expression where the
  neighbor uses a statement `if`.
- Comment style alternates: some regions carry stage-narration prose, some
  carry the original helper's rationale verbatim, some are nearly bare.
- Guard forms differ: early `return`, `continue`, nested `if`, and
  post-hoc `is not None` wrapping all occur.

## 3. Per-cluster record

### 3.1 `chalice/local.py` — `LocalGatewayAuthorizer.authorize`

**What changed.** The method previously delegated through a small chain:
route resolution, JWT interpretation for the Cognito case, authorizer-event
translation, and policy verification each lived below it. All of those
regions now appear directly in the method body: the route lookup with its
`KeyError`-tolerant access pattern, the JWT payload-segment split with
manual base64 urlsafe padding (the remainder calculation and `=` fill are
literal code now), the claims extraction with its machine-to-machine
warning path, the mutable auth-context update (which now occurs at two
separate regions of the body, one per producing path), the method-ARN
assembly with its leading-slash and query-string normalization, the
TOKEN-type authorizer event construction with its `NotAuthorizedError`
path, and the Allow/Invoke policy verification with inline regex
translation of `?`/`*` wildcards and a first-match break loop.

**Why this site and form.** The authorizer is the most branch-rich entry in
local mode and its helper chain was the deepest (route → token →
translation → verification), so it models the smell at its worst. Keeping
the auth-context mutation duplicated at both producing regions (rather than
factoring a tiny shared region) reflects the copy-then-patch evolution being
modeled: the second occurrence was pasted during a later fix, and the
duplicated region is exactly the kind of artifact that stage produces.

**Production role.** This code decides whether `chalice local` admits a
request at all: it maps real HTTP requests onto the authorizers the app
declared, so local testing behaves like a deployed API for builtin and
Cognito authorizers, and it produces the 401/403 error surfaces local mode
shows for denied requests.

### 3.2 `chalice/local.py` — `LocalGateway.handle_request`

**What changed.** The request-serving entry previously delegated context
construction, event synthesis, and preflight handling below it. Those
regions now sit inline: the Lambda timeout/memory synthesis, the
route-match-based event assembly (headers lowercasing, optional
multi-value query parameters, binary content-type detection with base64
encoding and the isBase64Encoded flag), the `ValueError` → `ForbiddenError`
translation with its full two-case message construction, and the OPTIONS
preflight region (allowed-method collection, the empty-CORS case, the
shared-CORS shortcut, and the manual `Access-Control-Allow-Methods`
completion). The bare `if method == 'OPTIONS'` early-return shape of the
preflight was chosen over nested extraction precisely because it keeps all
preflight decisions at one indentation, the way a hotfix would.

**Why this site and form.** It is the sibling entry to 3.1 on the same
request path, which lets the diff show the same terminal state reached with
different idioms (no duplicated regions here; body text inherited nearly
verbatim, differing mainly in what each absorbed region guards).

**Production role.** This is the local-mode stand-in for API Gateway: it
converts an incoming HTTP request into the Lambda event shape and hands it
to the app, and its preflight region is what keeps CORS behavior of
`chalice local` aligned with a deployed API.

### 3.3 `chalice/awsclient.py` — `TypedAWSClient.update_function`

**What changed.** The three-stage function-update pipeline (code update,
configuration update, tag reconciliation) previously ran through a chain
of private methods, with retry/error-context and waiter logic one level
deeper. All of it is now literal regions of the entry: the code-update call
with its except-path that instantiates the Lambda error context and reports
the deployment error, the function-updated waiter invocation, the
configuration payload assembly (environment variables, runtime, timeout,
memory, role, tracing config, VPC config via the existing VPC helper,
layers) with the client-method retry wrapper kept by name, and the tag
reconciliation with `list_tags`, the untag set-difference loop guard, and
the differing-or-missing-tags comprehension feeding `tag_resource`.

**Why this site and form.** The deploy client is where the "keep it all
while debugging the staging bug" story is most credible: atomic
code-then-config-then-tags sequencing is exactly what an oncall bug in
serverless staging forces a developer to trace, and the absorbed regions
keep the original stage boundaries visible as stage-narration comment
lines (code, configuration, tags), as hotfixed code tends to.

**Production role.** This path runs on every redeploy of an existing
Chalice app: it replaces Lambda code, rolls the declared configuration
forward, and reconciles tags against what is on the account, surfacing
actionable error contexts when AWS rejects an update.

### 3.4 `chalice/package.py` — `SAMTemplateGenerator._generate_websocketapi`

**What changed.** The websocket API section of the SAM template was
assembled by a chain of section writers below the generator entry. The
entry now contains all sections inline: the API resource, the three
handler-loop integration and invoke-permission constructions (written as
one loop over the handler names rather than three repeated regions —
deliberately differing from the original's repeated-block idiom), the
route-key derivation with the connect/disconnect/default integration
targeting expressed as an if/elif/else chain (replacing the original
dict-get mapping — another idiom rotation), the deployment and stage
sections, the optional custom-domain section with its mapping resource,
and the output section (also rewritten as a suffix loop over the handler
names, preserving the exact output keys and the stage- interpolated
endpoint URL).

**Why this site and form.** Template generation is the most
data-construction-heavy pipeline in the diff, so it models terminal-state
bulk without any control-flow pyrotechnics of its own. Rotating its two
construction idioms (loop-over-names and if/elif targeting) shows the
collapse reaching code that was restructured rather than merely pasted.

**Production role.** This is what turns a websocket-enabled Chalice app
into a deployable WebSocket API: the integrations and permissions wire the
three Lambda handlers to the API, and the domain/output sections carry the
public surface (endpoint URL, handler ARNs/names) consumed by users and by
the old deployer parity contract.

### 3.5 `chalice/deploy/sweeper.py` — `ResourceSweeper.execute`

**What changed.** The sweep pipeline — indexing the plan's recorded
resources, diffing deployed resources against it per resource type, then
planning deletions — previously ran through a dynamic-dispatch chain (`mark
→ determine → per-type determine_* handlers → plan deletion`). The method
now performs all three stages in one body: the marking loop building the
resource-name index, the reversed-order walk over deployed resources with
the per-type staleness comparisons expressed as an if/elif ladder (s3 bucket
identity check as a conditional expression, the sns/sqs/kinesis/dynamodb
marker comparisons as statement guards, and the domain-name per-key api
mapping reconciliation as paired set comprehensions), the deletion-planning
walk with its handler-argument preparation and append-vs-insert placement,
and the `api_mapping` naming detection that selects front insertion. The
`getattr`-dispatched delete handlers and the plan-update routine were left
untouched on purpose (see §4), as was the class-level
`specific_resources` tuple, which now doubles as an explicit outer guard on
the ladder.

**Why this site and form.** This cluster models the deepest absorption in
the set: two dispatch mechanisms (resource-type `determine_<type>` handlers
and `delete_<type>` handlers) originally shared one entry; collapsing only
the `determine` half — while leaving the `delete` dispatch delegated —
produces exactly the asymmetric, half-evolved shape a hotfix lineage
leaves behind, and the elif-ladder-over-type is the natural inline form of
a collapsed `getattr` family.

**Production role.** The sweeper protects users from orphaned AWS
resources on redeploy: after the new plan is built it walks everything the
previous deploy recorded in `.chalice/deployed.json`, finds entries the
new application no longer references (including event sources whose
external physical identity changed, and individual api mappings removed
from a custom domain), and appends the matching deletion calls to the
plan in an order that keeps domain teardown behind its mappings.

## 4. What was deliberately *not* collapsed, and why

- `ResourceSweeper._update_plan`, the `delete_<type>` handler family, and
  `_default_delete` remain methods: the deletion handlers are reached
  through a `getattr` dispatch whose members are also exercised directly in
  isolation, and keeping one dispatch delegated while collapsing its
  sibling mirrors the partial-evolution story (§3.5).
- `LocalARNBuilder`, `ARNMatcher`, `RouteMatcher`, and
  `LambdaEventConverter` in local mode remain intact; the absorbed regions
  consult them, which is the shape the callers had before.
- `TypedAWSClient._create_vpc_config`, the retry wrapper, and the
  Lambda-error reporters remain shared methods; the absorbed config region
  calls them by name.
- In `package.py`, the Terraform generator's independently-owned chain of
  same-named sections was left entirely alone; the two generators are
  separate classes with separate ownership of the same role.

These retention decisions are part of the modeled evolution: real hotfix
collapses stop at whichever callee still has other callers or gets
exercised on its own, and the remaining delegation boundaries are where a
careful reader can reconstruct the original architecture.
