# Injection design record — rollout bookkeeping absorbed into the vectorized environment base

## Repository and revision

openai/baselines at `ea25b9e8b234e6ee1bca43083f8f3cf974143998`. All work is in the
Python layer: `baselines/common/vec_env` (the vectorized-environment package),
`baselines/common/cmd_util.py` (the environment-construction funnel used by the
training entry points), `baselines/run.py` (the generic training entry script)
and `baselines/acer` (an algorithm that depends on stacked observations).

## Maintenance motivation

The vectorized-environment layer grew the classic way. The package contains a
thin abstract transport contract (`VecEnv`, implemented by in-process,
subprocess and shared-memory backends) plus a zoo of thin wrapper classes that
add one rollout concern each: `VecFrameStack` (repeat recent observations along
the channel/last axis), `VecNormalize` (running-mean/std filtering of
observations and discounted returns, numpy or TF-backed), `VecMonitor`
(episode accounting with an optional results file), `VecExtractDictObs`,
`VecVideoRecorder`. Assembling a ready-to-train environment, however, is the
caller's job: `run.py` fetches a raw `VecEnv` from `make_vec_env` and then wraps
it by hand (`VecFrameStack(env, 4)` for atari, `VecNormalize(env, use_tf=True)`
for mujoco). Two pain points motivated this change:

1. **Assembly drift.** The correct wrapper order and the per-*env_type* wrapper
   set live in the entry script, not in the construction funnel. Any new
   backend entry point has to re-derive stacking depth, normalization mode and
   episode recording from tribal knowledge, and the atari and mujoco branches
   have already drifted apart.
2. **Per-episode results files are per-environment only.** Since
   `bench.monitor.Monitor` is applied to every individual environment inside
   `make_env`, a rank running `num_env` copies writes `num_env` monitor files
   plus the model checkpoints. The training curves we actually read and plot
   aggregate them afterwards. A single batch-level results file per rank -
   one row per finished episode across the whole batch, same format the
   results plotting already consumes - removes that aggregation step and was
   asked for during review.

The direction chosen was to make the one object every training loop already
holds responsible for the whole rollout setup: a `VecEnv` that takes
construction options for the bookkeeping a rollout needs (frame stacking,
running-statistics normalization, episode results), implements them itself, and
is handed to the algorithm fully configured. The reasoning recorded at the
time: the object handed to a training loop should be the single owner of all
rollout state, so there is exactly one place to look at during debugging, one
object to checkpoint next to, and no wrapper chain to reassemble per entry
point.

## Normal evolution being modeled

This kind of growth is ordinary in this repository's history: older baselines
revisions wired `VecMonitor` inside `make_vec_env` (batch-level episode files
were a real feature once), and the wrapper family predates the current
entry-point-assembled configuration. The change modeled here is a
re-centralization in the opposite direction of the current design: instead of
"thin base + compose wrappers at the call site", the maintainer moves the
features into the shared base class and turns the construction options into
the contract. Feature by feature it is defensible - each block is copied from a
well-tested existing wrapper - and the accumulated result is the kind of
facade/mediator base class that later shows up in every debugging session.

## Overall design

The injected change has five coordinated parts.

* **Absorption into the base class** (`vec_env.py`). `VecEnv.__init__` grows
  options `frame_stack`, `normalize`, `use_tf`, `clipob`, `cliprew`, `gamma`,
  `epsilon`, `record_episodes`, `results_filename`, `results_info_keywords`.
  Depending on which options are on, the constructor creates stacking state
  (`nstack`, `stackedobs`) and a stacked `observation_space`, numpy-or-TF
  running-stat stores, and episode accumulators plus an optional results
  file. The public `reset()`/`step_wait()` become a fixed pipeline: collect
  raw results, stack, normalize, record episodes. The absorbed logic lives in
  private helpers grouped in three commented sections, mirroring the code they
  came from.
* **Transport conversion to a hook protocol** (`dummy_vec_env.py`,
  `subproc_vec_env.py`, `shmem_vec_env.py`). The concrete backends stop
  overriding the public `reset()`/`step_wait()` and instead implement private
  transport hooks; the base class calls them from its public pipeline and
  interleaves the bookkeeping around them. Their constructors forward an
  arbitrary `**vec_options` to the base constructor.
* **Factory coupling** (`cmd_util.py`). `make_vec_env` grows the same options
  as keyword arguments, collects them into one options dict, and forwards it
  to whichever backend it builds. When episode recording is requested and no
  filename was supplied, it derives one from the configured logger directory
  using the same per-MPI-rank naming scheme as the existing per-environment
  monitor files (`<rank>.vecenv`, suffixed by the results writer).
* **Entry-point simplification** (`run.py`). `build_env` asks the factory for
  the finished environment in one call - atari-style (stacking depth 4 plus
  batch-level episode recording) and mujoco-style (TF-backed normalization
  plus recording) - instead of wrapping afterwards.
* **Consumer renegotiation** (`acer/acer.py`, `acer/runner.py`). ACER requires
  stacked observations and previously asserted on the wrapper type
  (`isinstance(env, VecFrameStack)`). Since the stacked observation pipeline
  is now a property of the base class rather than one particular wrapper
  type, the assertions become capability checks (`getattr(env, 'nstack', ...)`)
  that accept either construction path.

The interface change is one-directional and additive for callers: anything
that stepped a `VecEnv` before (including `VecEnvWrapper` subclasses and the
video recorder, which call `step_wait()` directly) keeps working, because the
public names still exist and still return the same tuple shape.

## Per-cluster rationale

### `baselines/common/vec_env/vec_env.py` — the base class

**Constructor option block.** One construction-time decision point: what the
environment does during a rollout is fixed at build time, which is the
natural lifecycle for this layer (environments are built once per training
process). The option set is deliberately heterogeneous in shape, mirroring
where each feature came from: a count option (`frame_stack`), boolean flags
(`normalize`, `use_tf`, `record_episodes`), scalar controller constants
(`clipob`, `cliprew`, `gamma`, `epsilon`) and a filename/tuple pair for the
results export. The raw pre-stacking space is kept as a separate attribute
because `DummyVecEnv` needs the unstacked space to size its observation
buffers; the (possibly unmodified) space is then published as the public
`observation_space` the policy will be built against.

**Stacking section** (`nstack`/`stackedobs` state and the two stack helpers).
Newest-frame-last channel layout, identical to `VecFrameStack`, so policy
convolution layouts and ACER's expectations are unchanged whichever
construction path was used. Episode-final environments restart from a zeroed
stack, matching the per-environment atari behavior. Absorbing this first was
attractive because stacking is pure array work with one piece of state and no
I/O - the smallest risk for the first step of the migration.

**Running-statistics normalization section** (stat-store init, moment
combination, filtering, per-step application). The state comes in two
flavors selected by `use_tf`: plain numpy dicts (the cheap path) or
TensorFlow variables so the statistics are checkpointed with the model -
the property mujoco training actually relies on. The
moment-combination algorithm is the standard parallel
mean/variance/count merge copied across with the same numerics. The TF
variables are created under the `ob_rms`/`ret_rms` variable scopes with the
variance variable named `std`, i.e. exactly the naming `VecNormalize`'s
`TfRunningMeanStd` used, so checkpoints stay loadable across the migration.
This cluster is the largest absorbed block and the reason the base class
needed `numpy`/`gym.spaces` imports and a session dependency.

**Episode accounting and results-file section** (accumulators, per-episode
summary, CSV writer setup, file teardown). Episode returns and lengths are
accumulated per environment; when an environment finishes, one row
(`r`, `l`, `t`, plus caller-selected info keys) is emitted. The file format
reuses the shape of the existing per-environment monitor files - a `#`
comment line carrying the start time, a CSV header, and one row per finished
episode - so the current results readers and plotting tooling read batch-level
and per-environment files identically. The results file is opened in the
constructor (so a misconfigured path fails at build time, not mid-training)
and closed in `close()`, which the base class already owned as the resource
lifecycle choke point.

**Public pipeline and private transport hooks.** `reset()` and `step_wait()`
stay the public contract - several components in and outside the package call
`step_wait()` directly - but they now interleave the three concerns in a
fixed order: stack raw observations, filter through the running statistics,
then account episodes from the filtered rewards. The concrete transports
provide only the raw interaction through private hooks, which makes the
bookkeeping unavoidable rather than optional and keeps one place where the
order is defined.

### `dummy_vec_env.py`, `subproc_vec_env.py`, `shmem_vec_env.py` — transports

All three follow the same shape: accept `**vec_options`, forward them to the
base constructor, and rename `reset`/`step_wait` to the private hook names.
Shmem's `close_extras` and reset-while-waiting path also switch to the hook
name. This uniformity is exactly what makes the conversion feel mechanical
during review: the diff for each transport says "I am now only a transport,"
and the shared-momentum of three backends converting together is what let
the change commit as one coherent step rather than a contested redesign.

### `baselines/common/cmd_util.py` — construction funnel

`make_vec_env` is the one place every backend entry point funnels through, so
it is where the options have to surface for the "one call, ready to train"
goal. The options are collected into a single dict and splatted into whichever
backend is selected; per-MPI-rank default filenames follow the existing
`<rank>.<name>` convention used for per-environment monitor files, carrying
the deliberate suffix `.vecenv` + the results extension to distinguish the
batch-level file from the per-environment ones beside it.

### `baselines/run.py` — entry script

Two configurations encode the current training recipes: atari-style
(frame-stack depth 4, batch-level episode recording) and mujoco-style
(TF-backed normalization, same recording). The imports shrink to what the
script still uses; the wrapper assembly block disappears from the entry point
entirely - the visible goal the whole change was after.

### `baselines/acer/acer.py` and `baselines/acer/runner.py` — capability consumer

ACER's stacked-observations precondition was historically expressed as a type
check against the stacking wrapper. With stacking now coming from the base
class, the same guarantee holds for any backend built with the option, so the
checks are relaxed to capability detection that accepts either construction
path. Keeping the assertion's failure message actionable (build with frame
stacking) preserves the guard's original purpose: catching
misconfigured training scripts before they produce silently worse policies.

## Production role served

After the change, the layer reads as a deliberate coordinator: build one
object with options, hand it to `learn()`. The training loop sees one owner of
all rollout state; the checkpoint next to it contains the normalization
statistics; the results directory gains a readable batch-level curve file. The
cost shows up where it always does in this shape of design: everything the
training loop does passes through one large class, every transport change has
to respect the base class pipeline, and every consumer (such as ACER) has to
detect capabilities instead of trading on types.
