# Diff description — target-cluster identity facts threaded as loose values

## Maintenance motivation

Rally keeps learning more about the cluster it benchmarks. Over recent releases the
set of facts describing *which* cluster a race targeted grew from the classic trio
(distribution flavor, version, build revision) to a six-value identity picture:
distribution flavor, distribution version, build revision, a target id (cluster name
for on-prem installs, project id conceptionally for hosted/serverless targets), the
platform the target runs on (`serverless`, `hosted`, or `on-prem`), and the auth type
Rally used to talk to it (`api_key` or `basic`).

Every consumer of these facts wants all of them at once: the race record persists them,
the race-control banner prints them, the driver needs them at the exact lifecycle point
when track preparation completes, and the cluster-environment telemetry device publishes
them as cluster-scoped meta-info. A series of small, individually reasonable
maintenance steps — adding one more fact here, copying an existing parameter list
there, extracting a helper "to keep the lists in sync" — is exactly how this area of
the code grew. This change-set models the natural end state of that growth: the six
identity facts are handled everywhere as unrelated individual values, and every
addition or correction to the group now has to be repeated at each site by hand.

## Modeled development evolution

The diff reconstructs a plausible maintenance history for this area, as it would have
happened in several small steps rather than one redesign:

1. The driver's engine setup grew code that derives the identity facts while the
   candidate cluster is still reachable (the REST API check), because the driver
   learns things there (cluster response headers, active client options) that no
   later component still has access to. Static default values (`oss`, unknown target
   facts) cover the paths where the cluster is never queried (explicitly skipped
   REST check, static responses, serverless with a public user).
2. Those values need to survive from engine setup until *track preparation* completes
   — a different phase, run by a different actor — so they were stashed on the main
   driver actor as sibling attributes, through a small module-level helper so the
   stash point has one name.
3. At track-prepared time the driver hands the facts to race control with a message;
   the message declares all six values in its constructor, in the same order used
   everywhere else, which is also the order the existing `attribute`-style message
   body already exposes.
4. Race control's benchmark actor unpacks the message and calls the coordinator with
   the six values as explicit keyword arguments; the coordinator in turn forwards
   them to the race, which gained its own recording method when that bookkeeping was
   moved onto the race model during an earlier cleanup of the coordinator.
5. Independently, the cluster-environment telemetry device re-derives the same six
   facts from its own client (the target cluster is queried again at benchmark
   start) and publishes them as cluster-scoped meta-info; the publishing step was
   extracted into a module-level helper so the documented convention — which keys,
   in which order, which ones are conditional — lives in one named place.

Each step is the kind of change reviewers wave through on its own. The cumulative
result is that the same six values are enumerated over and over, site by site.

## Overall design

The change touches four production modules along the preparation path and one
independently-deriving path:

- `esrally/driver/driver.py` — derivation of the identity facts during engine setup,
  the driver actor's memory of them, and the completion message that ships them to
  race control;
- `esrally/racecontrol.py` — the benchmark actor receiving that message and the
  coordinator recording the facts on the race;
- `esrally/metrics.py` — the race model's own recording step;
- `esrally/telemetry.py` — the cluster-environment device's independent derivation
  and the helper that publishes the facts as cluster meta-info.

Two design conventions were kept deliberately, because they are what a maintainer
would preserve to minimize risk:

- Parameter lists along the path spell every fact individually and share the same
  order, so a new fact can be added by copying an existing list. The docstrings on
  the two extracted helpers say this out loud ("the parameter list ... matches the
  one used by the other paths that pass these facts around, so it stays easy to
  keep them in sync") — reflecting the reasoning used when the helpers were added.
- Values are passed plain (strings, `None`s), no intermediate container is
  introduced anywhere along the path.

Behavior is meant to be preserved: the facts reach the same consumers with the same
values, precedence, order, and conditionality as before.

## Change rationale by location

### `esrally/driver/driver.py` — identity derivation during engine setup

`Driver.prepare_benchmark` previously only extracted the raw cluster info response
(`cluster_details`); identity facts such as build flavor and revision were picked out
later, down-stream, at the moment they were needed for the completion message. As the
fact set grew (target id, platform, auth type), that later re-picking had to touch
several consumers, so this change moves the identity resolution *into* engine setup,
next to the REST check that produces the data it depends on (`build_flavor`, build
`number`, `build_hash`, `cluster_name`, the `x-found-handling-cluster` response
header, and the active client options for the auth type).

Two shapes were considered for the fallback paths: re-deriving them later (rejected:
static responses and skipped checks cannot be re-queried later, the response object
is gone) and central defaults established up-front (chosen): flavor, version, and
revision start at `"oss"` — mirroring the existing fallback for manually compiled
builds — and target id/platform/auth type start unknown, overwritten only when the
REST check actually ran. The serverless special cases (overwriting the version
number, fetching the real build hash for operators) stay exactly where they were.

This site is the natural origin of the loose values: it is where the group exists as
data before it exists as an interface, and keeping the per-fact derivation together
in one block is what makes the rest of the path easy to extend one fact at a time —
which is how the area grew in the first place.

### `esrally/driver/driver.py` — the driver actor's target-cluster memory

The facts are derived during engine setup but consumed when track preparation
completes; those are different phases and different message flows, and the actor's
children (workers, task executors) need the raw cluster details in between. So the
values are stashed on the main driver actor.

`DriverActor.__init__` initializes the whole set of sibling attributes up-front
(unknown/`"oss"`-free placeholders — plain `None`s at that stage, since the actor is
constructed before setup) so the actor has a stable shape before track preparation
finishes, and `remember_target_cluster` became the one named place that writes the
whole batch. The helper is module-level rather than a method because the driver's
unit tests drive preparation through the `Driver` class with a stand-in for the
actor (a plain `Mock` and in one case the actor class itself), and a free function
that takes the recipient as its first argument keeps that stand-in usable — a
module-level function stored values on whatever object it is handed.

The helper takes the individual facts one-by-one rather than the raw cluster
response because by the time it is called, the response has been resolved into
identity facts; taking the resolved values keeps it callable from any future path
that produces the facts. Its parameter list mirrors the completion message's
(listing all six in the same order) — the keep-them-in-sync convention described
above.

`DriverActor._after_track_prepared` reads the whole set of attributes back out of
the actor, one-by-one, and constructs the `PreparationComplete` message from them,
keeping the message construction as a straight transliteration of the stored state.

### `esrally/driver/driver.py` — the completion message

`PreparationComplete` already existed with a per-fact constructor (its signature
predates this change-set and gains nothing new here); it plays its role in the
modeled evolution as the interface that fixes, once, the order in which every other
list along the path learned to spell the facts — call sites copy from it. The
doc-rich helper functions and the receiver deliberately quote it as the reference
point for the shared order.

### `esrally/racecontrol.py` — message receiver and coordinator hand-off

`BenchmarkActor.receiveMsg_PreparationComplete` unpacks the message's attributes
into an explicit keyword-argument call onto the coordinator. Writing the pass
through as keywords (rather than e.g. passing the whole message deep into the
coordinator) keeps the thespian message a passive data carrier and keeps the
coordinator's interface reviewable at a glance; each new fact shows up as one
keyword line in the receiver.

`BenchmarkCoordinator.on_preparation_complete` keeps its signature spelling every
fact and now only *delegates*: the direct attribute assignments onto the race that
used to live here moved onto the race model as `record_target_cluster` (see below),
so this method documents, at the coordinator level, which facts arrived and hands
them on. The tail of the method — storing the race initially (without results) and
printing the console banner — is untouched.

Race control is the converging point of the whole path: all four pipelines
(from-sources, from-distribution, benchmark-only, docker) end here with the same
facts, so the coordinator spelling all six values individually is the most
consequential copy of the list.

### `esrally/metrics.py` — the race model's recording step

`Race.record_target_cluster` moves the per-fact assignment from the coordinator
onto the race itself, so race bookkeeping lives with the race model and
`metrics.py` owns how races are described. The method declares the facts as
individual parameters (mirroring the coordinator's list) and keeps the update
semantics it inherited from the coordinator: flavor, version, and revision always
overwrite, while an unknown (`None`) target id, platform, or auth type preserves
the previously recorded value — important because the race object is created
*before* the cluster facts are known, and some pipelines carry no auth or platform
information at all.

Placing the recording on the model (rather than leaving it in the coordinator) is
the natural "cleanup" step of the modeled evolution: the coordinator slimmed down,
the race model learned how to complete itself, and the six-value list was copied
once more in the process.

### `esrally/telemetry.py` — independent derivation and meta-info publishing

The cluster-environment device runs *after* engine setup, at benchmark start, when
the driver's facts are out of reach — so it derives the same six facts itself, from
its own ES client's `info()` response and its client options. `ClusterEnvironmentInfo.
on_benchmark_start` keeps the derivation (including the revision override for
serverless builds, the hosted-cluster header detection, and the api_key/basic
auth precedence) but now forwards the six resolved values, one-by-one, to a new
module-level publishing helper.

`store_cluster_environment_info` was extracted because the publishing convention is
a piece of institutional knowledge: which meta-info keys exist, in which order they
are written, and which ones are conditional (`target_id` only when known,
`target_auth_type` only when recorded — `target_platform` always, since it is
always derivable). Having that convention in one named function — instead of
inlined in the device — makes it reusable from any future device that learns the
same facts, and its parameter list again copies the shared order from the
preparation path "to keep them in sync", as its docstring records.

This site is deliberately the path's mirror image: the same facts, re-derived from
scratch on a different client, spelled out as unrelated parameters once more, with
the parameters of the two sites in one-to-one correspondence.

## Production role summary

The four modules touched are all long-lived, load-bearing parts of every race:
engine setup and verification (`driver.py`), race orchestration
(`racecontrol.py`), the persisted race record (`metrics.py`), and the telemetry
pipeline that feeds dashboards (`telemetry.py`). All pipelines and both retrieval
paths (query the cluster; fall back to `oss`/unknown) pass through the changed
code, and the change-set preserves each site's observable commitments — recorded
meta-info keys/order/conditionality, record-first-then-store sequencing, and the
console banner — while leaving the identity facts themselves spelled out as loose
individual values everywhere they travel.
