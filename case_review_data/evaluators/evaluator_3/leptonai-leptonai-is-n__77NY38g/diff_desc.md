# Injection design record — leptonai workspace-resource capability contracts

Task: `leptonai-leptonai-is-n` · Repository: leptonai/leptonai at `7c1e61a`
· Language: Python · Smell category: interface segregation (fat interface)

## Maintenance motivation

The v2 SDK grows a new resource API every quarter (endpoints, devpods, ray
clusters, dedicated node groups, storage data sources...), and the `lep` CLI
keeps re-implementing the same per-resource command flows — list, inspect,
watch readiness, tail logs, stop / restart — with only the route names
differing. Reviews of the CLI modules keep finding near-duplicated loops that
special-case each resource by hand, and every new resource re-decides which of
those flows it supports in an ad-hoc way. A maintainer proposes to centralize
the notion of "what a workspace resource can do" in the SDK itself so shared
tooling can be written once against a common contract.

## Normal evolution being modeled

This is the way such a refactor realistically lands in one reviewable change:

1. extract the recurring operation families (health inspection, runtime
   visibility, state control, the fine-tune model catalog) into abstract
   contracts in a new `resource_contracts` module;
2. add a single umbrella contract that says "a workspace resource", because a
   generic driver wants one type to walk every resource — the union of the
   families, not the intersection;
3. add per-category "profile" layers (workload resources vs configuration
   resources) that decide once how a category serves families it has no route
   for, so that the concrete leaf classes do not each invent that policy;
4. rehome every existing resource API under the umbrella so the union type is
   total over the workspace surface;
5. where a category default cannot cover a specific resource's reality, have
   that resource supply its own body so construction of the workspace client
   keeps working — the client instantiates every resource API on its named
   attributes, so every inherited abstract obligation must be resolved
   somewhere or the whole SDK fails to build.

Each step reviews fine in isolation: the contracts are honest ABCs, the
umbrella reads like a convenience, the profiles look like reuse, and the leaf
bodies are documented with the reasoning of the moment ("jobs run to
completion", "devpods expose no replica route", "a generic dashboard should
still render"). The design consequence is that every workspace resource now
inherits the full, merged operation surface; resources that support one family
must also accept the obligations of the other three, resolved either by
category profiles with silent empty/no-op bodies or by their own filler.

## Overall design

* `leptonai/api/v2/resource_contracts.py` (new): four capability-family
  ABCs — `ResourceHealthAPI` (readiness / termination detail),
  `ResourceRuntimeAPI` (replicas / log / events), `ResourceLifecycleAPI`
  (stop / restart), `ResourceModelCatalogAPI` (fine-tune models / trainers) —
  plus the `WorkspaceResourceAPI` umbrella that inherits all four, and two
  profiles: `WorkloadResourceAPI` for workload-shaped resources and
  `ConfigurationResourceAPI` for pure configuration resources.
* The eleven existing workspace-resource API classes are rehomed from the bare
  transport base (`APIResourse`) onto the umbrella through the appropriate
  profile: workloads (deployment, endpoint, devpod, pod, job, fine-tune) under
  `WorkloadResourceAPI`; configuration resources (secret, ingress, template,
  raycluster, dedicated node group) under `ConfigurationResourceAPI`.
* Where the landing surface has no route for a family, the umbrella's abstract
  obligations are resolved with documented default bodies: the profiles decide
  for whole categories, and individual resources (job, fine-tune, devpod, pod)
  supply their own when their reality differs from the category default.
* The client registration path is untouched: `APIClient` still constructs every
  resource API on the same attributes; the new bases only sit above the
  existing transport base, and every previously public method keeps its name,
  signature shape, routes and response parsing.

## Per-cluster rationale

### 1. Capability family contracts (`resource_contracts.py`, new module)

Four small ABCs declare the operation families the CLI keeps duplicating:
`ResourceHealthAPI.get_readiness/get_termination`,
`ResourceRuntimeAPI.get_replicas/get_log/get_events`,
`ResourceLifecycleAPI.stop/restart`,
`ResourceModelCatalogAPI.list_supported_models/list_trainers`. This is the
honest part of the design: each family is exactly the set of same-shape
operations the platform actually exposes for *some* resources, typed with the
v2 model types (`ReadinessIssue`, `DeploymentTerminations`, `Replica`,
`LeptonEvent`, `Iterator[str]`, `FineTuneModelInfo`, `TrainerInfo`).

Why this module and shape: it sits beside the leaf APIs under
`leptonai/api/v2/` so the contracts import the existing type modules and every
resource can subscribe without a new dependency layer; ABCs (not protocols or
registration hooks) match how the codebase already models "must provide"
obligations and keep static checkers useful to callers.

Production role: the shared vocabulary that generic per-resource tooling
(e.g. a future `lep resource status/log/stop` driver) can be written against.

### 2. The umbrella contract and the two profiles (`resource_contracts.py`)
`WorkspaceResourceAPI` inherits all four families and implements none of
them ("a workspace resource is anything that can be inspected, watched, driven
and listed from the catalog"). Because a union type must be instantiable per
resource, the two profiles below it decide the category defaults:

* `WorkloadResourceAPI` (deployment-shaped resources): implements the catalog
  family with empty lists — workload resources are not fine-tune catalogs —
  and leaves the health / runtime / lifecycle families abstract, because real
  workloads genuinely differ on those.
* `ConfigurationResourceAPI` (secret, ingress, template, raycluster, dedicated
  node group): implements all nine inherited obligations with silent
  "nothing to report" bodies — empty `ReadinessIssue()` /
  `DeploymentTerminations()`, `[]` for replicas / events / catalog models /
  trainers, `iter([])` for the log stream, and no-op `stop`/`restart` — on the
  reasoning that a generic dashboard walking all workspace resources should
  render an empty row rather than fail.

Why an umbrella plus profiles instead of per-resource subscription: the
proposer wanted one type to mean "any workspace resource" for generic tooling,
and category profiles centralize the "how do we degrade a family this kind of
resource never has" policy instead of re-deciding it in each leaf. The
consequence is that fidelity of a capability body is no longer discoverable
per resource: subscription states "supports", while the body quietly states
"no-op" or "empty".

Production role: `WorkspaceResourceAPI` is the type generic tooling targets;
the profiles are the two real resource categories the SDK ships today.

### 3. Legacy workload subscribers (`deployment.py`, `EndpointAPI` in
`endpoint.py`)

`DeploymentAPI` and `EndpointAPI` already implement the entire health, runtime
visibility and lifecycle surface with real requests (`self._get`/`self._put`
to `/deployments` / `/endpoints` routes), and their readiness/termination
degradations for the flag-on path are pre-existing documented behavior
(`NewEndpointAPIUnsupported`, plus a pod-flavoured delegation through
`self._client.pod`). Resubscribing them under `WorkloadResourceAPI` is a
one-line base change plus the import; they inherit the catalog family's empty
profile bodies.

Why included: these are the highest-traffic resources and the natural first
subscribers of the uniform contract; keeping their genuine implementations
untouched (only the class bases move) preserves the request paths exactly.

Production role: the legacy deployment path and the flag-on endpoint path both
remain the fully capable workload implementations of the contract.

### 4. Flag-on and legacy pod paths (`devpod.py`, `pod.py`)

`DevPodAPI` and `PodAPI` genuinely drive state (`stop`/`restart` via the
devpod `spec.stopped` switch and restart route, or by delegating through the
client's legacy deployment path), but neither surface exposes a
replica-list or event route: a devpod/pod is a single node, not a scaled-out
workload. Their base classes move to `WorkloadResourceAPI`, and each supplies
the runtime-visibility bodies its platform reality dictates: `get_replicas`
and `get_events` return documented empty lists ("a generic replica view
simply finds no replica rows to render"), while the pre-existing honest
degradations (`get_readiness`/`get_termination`/`get_log` raising
`NewDevPodAPIUnsupported`, pod log tailing via the deployment API) stay
exactly as they were.

Why this site: the two paired implementations of the pod surface
(LEP-5665 flag-on `/devpods` vs the legacy pod path) must stay
behaviorally identical across the mode switch, so both take the same
subscription step and the same documented empty-visibility defaults.

Production role: keeps both pod paths constructible under the umbrella while
preserving their degradation semantics for unsupported sub-operations.

### 5. Batch workload subscribers (`job.py`, `finetune.py`)

`JobAPI` and `FineTuneAPI` are submit-and-finish resources: no
`/readiness`-style inspection route, no lifecycle route (a job runs to
completion; `stop`/`restart` would contradict the model), while their log /
replica / event moment exists only on the job surface (`JobAPI.keep`s its
real implementations). They move under `WorkloadResourceAPI` and resolve the
remaining obligations themselves: `get_readiness`/`get_termination` return
empty models ("the unified readiness view over jobs is intentionally empty
rather than unsupported, so generic dashboards keep rendering"), and
`stop`/`restart` raise with a refusal message ("jobs run to completion;
delete the job if it must be interrupted"). The fine-tune API keeps its
genuine catalog implementations (the only real subscriber of
`ResourceModelCatalogAPI`) and fills the seven workload-family obligations in
the same two documented styles (empty returns for views, explicit refusal for
lifecycle).

Why this site: the batch surfaces are where the catagory defaults of
`WorkloadResourceAPI` are most visibly wrong, so the leaf-level resolution
policy is spelled out per family — the module notes say a generic driver
should see "empty" for inspection families, but the reason for the refusal on
lifecycle families must be surfaced rather than guessed.

Production role: batch resources become walkable by the same umbrella; their
inability to serve a family is now encoded as quiet emptiness inside a
capability that the type claims they have.

### 6. Configuration resources (`secret.py`, `ingress.py`, `template.py`,
`raycluster.py`, `dedicated_node_groups.py`)

Secrets, ingresses, templates, ray clusters and dedicated node groups are
pure configuration / catalog resources: they have no readiness detail, no
replicas, no live logs, no events, no stop/restart, and no role in the
fine-tune catalog. Each is rehomed from the bare transport base onto
`ConfigurationResourceAPI`, which already resolves all nine obligations with
the silent category defaults. No leaf edits beyond the import and base class
are needed.

Why these five: they are the workspace resources whose entire surface is
create/list/get/update/delete-shaped, i.e. exactly the resources for which
every operation family of the umbrella is inapplicable, so the profile's
policy (empty views, no-op lifecycle) silently decides their capability
surface.

Production role: all configuration resources become instances of the one
umbrella type, so future generic tooling can enumerate "all workspace
resources" — while their real, route-backed methods (secret listing, ingress
endpoint management, cluster scaling updates, node-group volume and storage
permissions) remain untouched.

## Out of scope by design

Query-only services (log query, hardware-resource shape catalog), the pure
data model modules under `types/`, the translation layer, and the basic
per-resource data verbs (`list_all`/`get`/`create`/`update`/`delete`) keep
their existing shapes and are not part of this change. The CLI modules and
`APIClient` registrations are not modified: the new bases sit above the
existing transport base, and no behavior of an existing method changes.
