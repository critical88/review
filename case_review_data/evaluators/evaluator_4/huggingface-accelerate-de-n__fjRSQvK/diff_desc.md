# Injection design record — expired FSDP2 conversion compatibility windows

## Maintenance motivation

Accelerate v1.15.0 staged the FSDP2 conversion: the `fsdp_version=2` plugin layout
(`reshard_after_forward` and the reworked field surface) became the default while launchers,
configs, and checkpoint layouts written against the pre-v1.15 argument surface were still in
the field. Conversions in this codebase are staged rather than cut over, so the v1.15 line kept
several bounded compatibility windows — normalization arms, alias acceptance, re-emission of
removed variable names, layout rebasing — each intended to be held open for one release and
then dropped.

The v1.16 line is the second release on the new default. The realistic maintenance failure this
case models is what happens when those windows expire on paper but not in the tree: the arms
stay behind, disabled by markers that will never change again, together with every helper that
existed only to serve them, because each deletion is deferred one more release ("keep the table
with its resolver until both are deleted together", "keep the marker until the flattener is
deleted alongside it"). The tree then carries a small archaeology of the conversion: guards
that resolve to constants, limbs that can no longer contribute behavior, and a shared
compatibility surface whose only remaining role is documentation.

## Evolution being modeled

The diff simulates the trailing edge of that staged rollout, as if each window had been
retired in-place at its expiry, by different authors over time, rather than deleted:

- some windows were closed with a bare flag binding in the module that owns the path;
- one used an inverted "completion marker" for a migrate-on-load path, which stayed behind in
  its `True` orientation;
- one used a window-predicate helper so the staging flag read like a policy question;
- the widest window — the pre-v1.15 plugin field surface — outgrew a flag and grew a shared
  module with a registry of per-field adapters, used from more than one path;
- and one genuinely supported compatibility behavior survived the cleanup, so the final state
  contains a live, environment-driven opt-in sitting next to retired material that looks
  almost identical.

That unevenness is deliberate: real conversion residue is not planted in one shape, and its
retirement cannot be one mechanical decision.

## Overall design

Six production files carry the change set. Four of them host one expired arm each, in different
subsystems (plugin configuration, launcher environment preparation, state bootstrap, checkpoint
loading, dataloader preparation); the arms are locally plausible for their host path — each one
quotes the compatibility job that path actually did during the conversion. The fifth file,
`src/accelerate/utils/_fsdp_compat.py`, is the shared retirement surface: one new module that
holds the widest window's field adapters, the adapter class that applies them, the launcher
variable re-emission helper, and the conversion-window predicate that two other files import to
guard their arms. The sixth edit is the launcher's live opt-in re-emission, which is real,
supported behavior kept in the tree next to the retired arm it resembles.

The arming mechanisms deliberately vary, because the retirements they model would have been
closed by different people with different habits:

- a bare module flag bound to `False` in the state bootstrap (**G1**, `state.py`):
  `if _LEGACY_BACKEND_ALIASES_ACCEPTED:`;
- a zero-argument window predicate returning `False` (**G2**, `data_loader.py`):
  `if _legacy_stateful_sampler_window_open():`;
- an inverted completion marker: the helper returns `True`, the arm negates it
  (**G3**, `checkpointing.py`): `if not _rng_layout_migration_complete():`;
- a layout-tag comparison behind an imported gate (**G4**, shared by
  `utils/dataclasses.py` and `utils/launch.py`): `_fsdp_legacy_sharding_window_open()`
  compares two module constants describing different layouts, so the guard reads like a
  configuration check while still resolving to a fixed outcome.

Every arm's body is written as it would have been during the window: it consults only real,
current argument and object surfaces (e.g. `getattr(args, ...)` on the launcher's actual
field names, `setattr` on the plugin instance), so nothing inside an arm betrays the arm as
synthetic. The imports that bind the arms' helpers into live files are ordinary in-style
imports, sorted as the surrounding import blocks sort.

The support cast behind the arms also varies in shape: a resolver helper plus a mapping table
(`state.py`), a plain mutating helper (`data_loader.py`, `checkpointing.py`), and — behind the
shared gate — a registry dict of per-field adapter functions, an adapter class with an
`__init__`/`adapt`/`supported_fields` trio, a constant mapping table, and two entry helpers.
The registry-driven shape matters to the design: reaching the deadness of the per-field
adapters requires following the registry through the class, not just a call site.

## Per-location rationale

### `src/accelerate/utils/_fsdp_compat.py` (new module)

The conversion's widest surface needed a home that more than one path could share, which is
how compat shims in this codebase are actually organized. The module's public-facing story is
its docstring: it exists to normalize pre-v1.15 field spellings and re-emit removed launcher
variables, and says plainly that the window has expired. Internals: two layout-tag constants
whose comparison is the window predicate (**G4**, so the gate function reads as if it answered
a configuration question), the textual-choice mapping table used by the `sharding_strategy`
adapter, three per-field adapters (strategy, backward-prefetch alias drop, offload spelling),
the field→adapter registry dict, the adapter class that applies the registry in place, and two
entry helpers (`_adapt_legacy_plugin_args` for plugin-shaped callers,
`_propagate_legacy_fsdp_fields` for launcher-shaped callers). All names are private; the
module interacts with callers only through the two imported symbols in each consumer.

### `src/accelerate/utils/dataclasses.py`

`FullyShardedDataParallelPlugin.__post_init__` is where pre-v1.15 plugin fields would have
been normalized before the v1.15 checks ran, so the arm sits immediately after the env prefix
is established and before version resolution — exactly the spot a bridging step would occupy.
It is guarded by the imported **G4** gate and calls the module's plugin-shaped entry helper
with `self`. The import line binds both the gate and the entry helper, placed in the local
import block according to the file's existing sort order.

### `src/accelerate/utils/launch.py`

Two mechanisms with nearly identical vocabulary live close together, which is the point of
this site:

- a retired arm at the top of the `args.use_fsdp` section of `prepare_multi_gpu_env`, guarded
  by the same imported **G4** gate, re-emitting the removed per-field variable names through
  the module's launcher-shaped entry helper;
- a live, still-supported opt-in (`_legacy_sharding_reemit_requested`, guarded by an
  environment variable read through `str_to_bool`, with the variable name and the affected
  DeepSpeed range documented in its docstring) that re-emits the flat legacy sharding variable
  for clusters pinning old DeepSpeed. This one kept its runtime input source on purpose: the
  surrounding narrative (compat re-emission, legacy variable naming, one-more-release rollout
  note) is shared with the retired arm, so the two must be told apart by how they are armed —
  by their reachability, not by their vocabulary. The docstrings and release-note framing give
  the supported one a real, checkable justification for existing.

### `src/accelerate/state.py`

`PartialState._prepare_backend` probes and names the distributed backend; during a chained
probe rework it would plausibly have accepted the earlier alias spellings for one release.
The arm sits right after the probe chain completes, before the device-type fallback chain,
and resolves the (already canonical) probe result through a module-local alias table — the
kind of pass-through that makes the vestigial nature of the arm visible only under scrutiny.
The arm is a bare-flag guard (**G1**: `_LEGACY_BACKEND_ALIASES_ACCEPTED = False`, with a
comment saying the table stays "with its resolver until both are deleted together"), the
shape a window gets when the owner disables it in the same file. Table and resolver helper are
module-local so the arm, its flag, and its table read as one self-contained retirement.

### `src/accelerate/checkpointing.py`

`load_accelerator_state` reads per-rank RNG entries; the v1.15 RNG-state layout rework is the
kind of persistence change that ships with a migrate-on-read fallback for old checkpoints.
Here the fallback got an inverted arm (**G3**): `_rng_layout_migration_complete()` returns
`True`, the arm runs `if not ...complete()`, and its docstring says the marker stays until
the flattener is deleted alongside it — the exact deferral that leaves both behind. The
flattening helper itself (`_flatten_legacy_rng_states`) is written against the current
entry shape (`states.pop("rng_state", ...)`, `setdefault` for the current key names), so the
fallback would have been functional during its window. The arm sits between the pickle load
and the `step` read it used to make layout-agnostic.

### `src/accelerate/data_loader.py`

`prepare_data_loader` is the dataloader-side participant in the staging of
`use_stateful_dataloader`: round-tripped dataloaders could arrive pre-wrapped in the staging
shim, so during the window the sampler is re-wrapped before the sharding logic inspects it.
The arm sits directly after `get_sampler(dataloader)` and before the seeded-sampler check —
the position where the first wrapper generation would be resolved. It uses a window predicate
(**G2**, `_legacy_stateful_sampler_window_open`), and its helper mutates the real sampler
surface (`_legacy_stateful_inner`, `dataset`) the way a two-generations-of-wrappers fix would.

## Deliberate structural variation

- **Four distinct arming mechanisms** (bare flag; zero-argument predicate; inverted completion
  marker; cross-module layout comparison) so the same retirement pattern cannot be recognized
  by one syntactic habit.
- **Support-cast morphology ranges over** plain functions, a resolver+table pair, a mutation
  helper, a registry dict feeding an adapter class, a constant mapping table, and one whole
  module — the deadness question looks different at each.
- **One shared gate imported by two files**, so a decision made on one path (deleting the
  shared module) constrains what another path's repair can keep.
- **A live/dead near-miss pair in the launcher**, built to force reachability reasoning rather
  than vocabulary matching: one arm answers to a fixed condition, its neighbor answers to the
  environment at runtime.
- **Limb bodies touch only real argument and object surfaces**, so they read as production
  code, not as placeholder blocks.

## What the change set does not touch

No public name, signature, environment variable, output, or warning is altered anywhere. The
existing FSDP2 default layout, the deprecation-warning machinery around the conversion, backend
probe results, checkpoint layouts, and dataloader sharding behavior are exactly as the base
tree defines them; every guarded limb is written to be inert and every live path is written to
behave as before.
