# Injection design record — convenience-API accumulation on the model object

This record documents the design of the injected change: the maintenance story it models,
the shape it took, and the reason each location was selected. It describes intent, not
required outcomes.

## Maintenance motivation

TensorRec's model object is the package's one famous class: notebooks, the quickstart, and
the getting-started blog post all revolve around constructing it, fitting it, and reading
its predictions. A plausible maintainer pressure follows directly from that fame: every
time a user struggle shows up in an issue or a demo needs one less import, the cheapest
fix is "put it on the model object," because that is the object everyone already holds in
scope.

The change models that pressure taken seriously over several maintenance passes, aimed at
a "one import, one object" story: accept input in whatever form the user already has it
(a sparse matrix, a matrix in a list, a TensorFlow Dataset, a TFRecord dump), handle the
loss sampling and the batched-regression knob automatically, fit, and then immediately
report the metrics the README grid reports — all without leaving the object, and without
asking the user to manage a TensorFlow session.

Each individual step in that direction is a defensible quick win. The recorded design
deliberately keeps the accumulator's perspective: it never deletes a public import path
(hand-compatibility first), it never renames graph or iterator names (saved models made
last week must keep loading), and it leaves a compatibility shim everywhere a caller
might exist.

## Modeled development evolution

The change is written as the natural accumulation of four maintenance passes rather than
one big redesign:

1. **Input convergence.** "Users keep converting their data to fit our API" is answered by
   letting the model accept raw input. The conversion, iterator construction, and
   dimension probing that used to live in the input/utility helpers are duplicated onto
   the model so the object can digest whatever it is handed.
2. **Training ergonomics.** Power-user fitting knobs are surfaced on the object: the
   stochastic item sampling bound into the loss and the batched-regularization scaling
   are inlined as small model methods so advanced fitting behaviors feel like model
   configuration.
3. **Quickstart metrics.** A one-line `fit`-and-score workflow is added for demos. The
   rank-metric arithmetic moves onto the model with it — the deciding argument in the
   modeled review discussion being "one source of truth on the object" — and the
   evaluation module keeps its public function names as thin delegates so old snippets
   keep running.
4. **Session ergonomics.** Interactive users kept tripping over session management, so
   the shared session becomes model state with `get_session`/`set_session` classmethods.
   The old session-management module stays as an alias for the same behavior and its
   imports the model class lazily, inside the function bodies, because the model module
   now transitively imports the session module at load time and a top-level import would
   not resolve.

Pass 4 is where the accumulation crosses a load-order constraint: because the model file
still imports its error/util/input chain at import time, and that chain reaches the
session helpers, the session helpers can no longer import the model at module level
without breaking the package import. The lazy in-body import is the modeled maintainer's
workaround of choice (deferring the import until the first call), a familiar pattern that
keeps the feature shipping without forcing the maintainer to question the accumulation
itself.

## Overall design

The transformation is cross-file and behavior-preserving:

- **`tensorrec/tensorrec.py` — the model object.** The class keeps its constructor,
  graph-construction, fitting, prediction, and save/load responsibilities and grows four
  absorbed responsibility areas: input/dataset preparation, training-auxiliary math,
  rank-evaluation analytics, and session ownership. Absorbed code is added as declared
  methods/staticmethods on the class; the class stops importing the helper modules it
  duplicated. The original lifecycle methods are rewired to consume the absorbed copies
  so behavior is unchanged from the outside.
- **`tensorrec/eval.py` — the evaluation module.** Fitting-and-scoring moves onto the
  object as a model method; the module's metric functions retain their names and
  signatures but become pass-through wrappers into the class, and the dataset-level grid
  and random-rank evaluation routines route through the object as well.
- **`tensorrec/session_management.py` — the session module.** Session state moves to the
  class; the module functions survive as forwarders with call-time imports of the model
  class.

Public import surface, signatures, graph/iterator names, numerics, and the test suite's
observable behavior are all preserved by construction; deleted helper imports inside
`tensorrec.py` are replaced by the absorbed bodies and no public symbol is removed.

## Per-location design

### tensorrec/tensorrec.py

**Module imports** — `math` and `six` are added (needed by the absorbed batched-alpha and
sampling bodies); imports of the input, session, and util helper modules are dropped
because their consumers now live inside the class.
*Why here:* the import block is the honest ledger of what the class still depends on
after absorbing the helpers; removing the module imports is what makes the absorption a
real ownership transfer instead of a second copy sharing the first.

**Class attribute `tf_session = None` and classmethods `get_session` / `set_session`**
— the shared session becomes class state managed by the model object, mirroring the
original module-level `_SESSION`/accessor semantics (one session, `None` resets it).
*Why here/shape:* session ownership is a single small stateful concern, so a class
attribute plus two classmethods is the minimal shape; classmethods (not instance methods)
keep the "one session per process" semantics that the module previously enforced.
*Production role:* interactive users fit and predict without importing a session
module; class state guarantees the whole package sees one session.

**`_create_input_iterator`, `_create_dataset_from_sparse_matrix`,
`_create_dataset_from_tfrecord` (with nested record-parsing helper),
`resolve_input_datasets`, `_get_dataset_dimensions`** — the input/dataset pipeline stage
absorbed from the input/utility helpers: iterator construction with the package's shared
iterator names, sparse-matrix→Dataset and TFRecord→Dataset conversion, raw-input
resolution for all four accepted input forms, and fit-time dimension probing.
*Why here:* every one of these was invoked by the model's own fit/predict call paths, and
the modeled maintainer wanted `raw_input` digestible directly by the object; hoisting
them onto the class (and rewiring the fit/predict plumbing to call `self.…`) is what
makes "hand the model anything" true.
*Production role:* converts user-held data into the Datasets/iterators that the graph
consumer needs; dimension probing declares the TensorFlow side of the graph while
building.

**`_sample_item_indices` (staticmethod) and `_calculate_batched_alpha` (staticmethod)**
— the training-auxiliary stage absorbed from the math helpers: the stochastic item
sampling that sampled loss functions template bound via `functools.partial`, and the
batched regularization scaling that user-batched fitting consumes.
*Why here:* both are pure functions already consumed exclusively by fitting code inside
the class, so moving them is low-friction; keeping them staticmethods preserves their
call shapes.
*Production role:* sampling feeds the loss's negative examples; batched alpha scales the
L2 loss across user batches.

**`fit_and_evaluate` (instance method) and `_setup_ndcg` / `_idcg` / `_dcg` /
`precision_at_k` / `recall_at_k` / `ndcg_at_k` / `f1_score_at_k` (staticmethods)** — the
rank-evaluation stage absorbed from the evaluation module, plus a fit-then-score
orchestration method (`fit_kwargs`, recall/precision/NDCG at configurable k) added as the
quickstart one-liner.
*Why here:* the modeled argument was that quickstart numbers should come from "the same
code the user just ran", so the metric arithmetic was moved next to fitting and the
module functions were left as delegates. Staticmethods keep the raw-rank metrics usable
without a fitted model (needed for baseline comparisons). The three NDCG helpers travel
with `ndcg_at_k` because they are its private sub-steps.
*Production role:* computes precision/recall/NDCG/F1 from predicted ranks against a test
interaction matrix; `fit_and_evaluate` backs the demo grid.

**Rewired lifecycle internals — `_attach_graph_hooks`, `_build_input_iterators`,
`_build_tf_graph`, `_create_datasets_and_initializers`, `fit_partial`, the eight
`predict*` methods, `save_model`, `load_model`** — the existing model-lifecycle bodies are
edited to consume the absorbed members: prediction/serving reads the session via the
class accessor, iterator construction and dataset resolution call the in-class copies,
sampling is partial-bound from the class staticmethod, batched fitting uses the in-class
dimension probe and alpha calculator, and save/load run against `get_session()`/class
state (with `load_model` reached through the class, since saving and restoring sessions
must agree with how they will be used later).
*Why here:* these call sites are what make the absorbed methods load-bearing rather than
dead copies; rewiring them (rather than leaving helpers in place) is what completes the
ownership transfer while preserving observable behavior — the same iterators with the
same shared names, the same `py_func` sampling partials, the same save/reload round trip.
*Production role:* unchanged — this is the fitting/serving/persistence surface of the
package; only its supplier changes.

### tensorrec/eval.py

**`precision_at_k`, `recall_at_k`, `ndcg_at_k`, `f1_score_at_k`, `_setup_ndcg`, `_idcg`,
`_dcg`, `fit_and_eval`** — each keeps its exact public signature and docstring contract
but its body is reduced to a delegate call into the model class. `fit_and_eval` gains the
default-`None` `fit_kwargs` handling the demo scripts showed.
*Why here:* the evaluation module is the donor whose logic moved in pass 3; keeping the
functions as wrappers preserves every downstream import (examples, README snippets,
external code) while the module itself stops owning its math.
*Production role:* the package's stable evaluation API.

**`grid_check_model_on_dataset`, `eval_random_ranks_on_dataset`** — the two higher-level
evaluation routines are rerouted to construct/follow the object: the grid loop fits and
scores through the model method, and the random-rank baseline computes its metrics
through the class statics.
*Why here:* these callers would otherwise drift from "the one source of truth on the
object" that pass 3 established; rerouting them is what the modeled maintainer would
consider consistency.

### tensorrec/session_management.py

**`get_session`, `set_session`** — module functions preserved with the original semantics
and re-pointed at the model class's accessors, importing the model class inside the
function bodies at call time.
*Why here:* pass 4 makes the object the session owner; the module survives as a
compatibility alias for the many scripts importing it. The in-body import shape is forced
by load order — the model module's import chain reaches this module at import time, so a
top-level import of the model class here would make the package unimportable; deferring
to call time is the minimal change that ships.
*Production role:* legacy import path for session access, now deferring to the object.

## Design notes

- Nowhere does the change rename a public symbol or rely on new names visible from
  outside; compatibility is maintained mechanically so that all existing imports, calls,
  and saved artifacts behave identically.
- The two donor modules were selected because they are the two areas whose logic the
  modeled maintainer actually relocated, and keeping (rather than deleting) their public
  functions matches how such accumulations are usually landed: deprecation is a later
  decision, and the easy path preserves the API while hollowing the module.
- The absorbed logic stays verbatim (including its parameter names and math) because the
  story is a change of address, not a rewrite; that verbatim quality is also what kept
  behavior preservation reviewable in review-by-diff terms.
