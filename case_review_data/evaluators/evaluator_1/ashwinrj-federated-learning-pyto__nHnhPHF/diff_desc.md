# Injection design record: explicit per-component experiment settings

## Realistic maintenance motivation

This repository runs two experiments over a shared set of components: a
federated simulation and a plain centralized baseline. At the clean revision,
every shared component received the entire parsed command-line namespace
(`get_dataset(args)`, `LocalUpdate(args, dataset, idxs, logger)`,
`CNNMnist(args=args)`, `test_inference(args, model, test_dataset)`,
`exp_details(args)`) and fished out the handful of settings it needed.

Maintainers of such a project typically grow uncomfortable with that style:
components cannot be exercised without building a full argument namespace,
readers must open the whole option parser to learn what a component actually
consumes, and every component silently depends on the option names. The
natural remedy - the one this injection models - is to peel components off
the namespace one at a time, each time declaring exactly the settings the
component consumes as explicit parameters.

Each individual change below is the kind of small, locally defensible commit
a real maintainer would write. The design deliberately models the dynamic in
which every boundary evolves its own explicit interface while no shared
abstraction ever coalesces, so the same groups of related setting names end
up declared repeatedly in signature after signature, and both entry points
end up thread the whole expansion through their call sites by hand.

## The evolution being modeled

Ordered as the series of commits this evolution plausibly took:

1. **Loader takes its facts explicitly** (`src/utils.py`, `get_dataset`).
   The dataset loader previously picked `data_dir` itself from the dataset
   name and read `num_users`/`iid`/`unequal` off the namespace. "Let the
   caller own where files live, and let the loader name the facts it uses":
   the signature becomes
   `get_dataset(data_dir_root, dataset, num_users, iid, unequal)`, where
   `data_dir_root` is the resolved directory the callers now pass in.
2. **Shared optimizer helper** (`src/utils.py`, `build_optimizer`). The SGD
   and Adam construction branches were duplicated between the local trainer
   and the plain baseline. The maintainer lifts the shared construction into
   a helper that names only the two settings it consumes - the learning
   rate and the optimizer choice (momentum keeps its existing default).
3. **Trainer takes its knobs explicitly** (`src/update.py`,
   `LocalUpdate.__init__`, `LocalUpdate.train_val_test`,
   `LocalUpdate.update_weights`). Instead of storing `self.args`, the
   trainer states the six local-training settings it consumes across its
   methods - local batch size, local epochs, learning rate, optimizer
   choice, verbosity and device - as explicit parameters, resolves the
   device at construction, and reads them from its own attributes
   afterwards. Its loader-slicing step, which previously read the batch
   size through `self.args`, now takes that one setting alongside the
   dataset and its index subset, which are genuine inputs rather than
   settings.
4. **Architectures name their shape** (`src/models.py`, the three
   convolutional constructors). Reaching into the namespace for the channel
   and class counts gave way to explicit shape parameters, applied
   uniformly to all three convolutional classes in one sweeping stylistic
   commit - including the members a given architecture ignores, since the
   maintainer is standardizing the three constructor interfaces rather than
   pruning each one.
5. **Evaluation mentions only what it uses** (`src/update.py`,
   `test_inference`). Of the namespace, only the device flag was consumed;
   the signature becomes `(model, test_dataset, gpu)`, taking the remaining
   arguments as explicit data.
6. **Report prints its facts** (`src/utils.py`, `exp_details`). While
   converting the last namespace consumer, the maintainer declares the ten
   facts the report prints - model, optimizer, learning rate, global
   rounds, dataset, user count, IID flag, user fraction, local batch size,
   local epochs - explicitly, and adds the dataset name and the user count
   to the printed output so the run header is complete.
7. **Entry points thread the new interfaces** (`src/federated_main.py`,
   `src/baseline_main.py`). Both mains resolve the data directory mapping
   the loader used to own and assemble the explicit lists at every call:
   the trainer instantiations list all eight names (twice per round inside
   the federated training loop, plus once more per user for the accuracy
   pass), each convolutional construction lists three names, the loader and
   the report get their full lists once each.

The shipped test suite under `src/tests/` predates the injection as this
repository's component test baseline. It is included in the diff because it
is the project's standing behavioral contract - exercising component
behavior now observable from the code with no namespace or grouping
assumptions baked in. Its helpers resolve parameters from candidate pools,
so it stays agnostic to how the settings end up being threaded; the
behavior it pins (loader selection and directory, partitioning results,
local split sizes and loader configuration, optimizer construction, model
geometry, report content, and evaluation metrics) is precisely the behavior
the entry points must preserve while their components' interfaces churn.

## Overall design

The design's core property is the semantic grouping of experiment settings
into three roles that recur across the whole training side:

- **Dataset identity and distribution** - dataset name, user count, IID
  flag, unequal flag - declared by the loader and re-declared by the report.
- **Local training** - local batch size, local epochs, learning rate,
  optimizer choice, verbosity, device - declared by the trainer, overlapped
  by the report, and re-declared as a subset by the trainer's own
  loader-slicing step.
- **Model shape** - channel, filter and class counts - declared
  identically by all three convolutional constructors.

Because the groups are defined by meaning rather than by location, they are
re-declared independently at each boundary that consumes them, and the
identity of members is stable across sites, which maximizes the shared-name
overlap the repeated pattern exhibits. The two data items that only look
like they belong - the record/index pairs flowing into the loader-slicing
step, and the perceptron geometry triple, which is computed from the loaded
data - were left as regular parameters precisely because they are genuinely
different data: this preserves the discrimination work a reader must do to
tell settings groups from real inputs.

Warts of the original revision are deliberately preserved so the evolution
stays purely about parameter threading: the always-truthy
`elif dataset == 'mnist' or 'fmnist'` condition, the fallback where dataset
names other than `mnist`/`cifar` load plain MNIST data while directories are
keyed to the fashion variant, the untouched `gpu_id` device handling, the
contrastive `modelC` class, and the pickle/save bookkeeping in the
federated main are all kept structurally identical.

## Per-location notes

- `src/utils.py#get_dataset`: five explicit parameters; directory and the
  four dataset facts. Chosen because it is the pipeline's data entry point
  and the largest genuine data path; making the caller own the directory
  is exactly the kind of upstream push a maintainer makes, and the four
  facts reappear verbatim wherever the same information is needed.
- `src/utils.py#build_optimizer`: two parameters plus defaulted momentum.
  A pure de-duplication commit whose signature names only what both call
  sites already share - the natural seam for a helper, and the reason its
  group membership arises through the learning rate/optimizer names
  repeating in the trainer and, transitively, everywhere those settings
  flow.
- `src/utils.py#exp_details`: ten explicit parameters. A helper whose entire
  job is to enumerate the run's settings is the most plausible place where
  listing every consumed setting feels locally justified - printing a
  report means touching every member anyway.
- `src/update.py#LocalUpdate.__init__`: the broadest interface on the
  training side - three genuine inputs (dataset, index subset, logger)
  plus the six local-training settings. It is the object most likely to
  gain new knobs, which makes its explicit interface both the most tempting
  and the most expensive to keep re-declaring.
- `src/update.py#LocalUpdate.train_val_test`: the subset declaration
  (dataset, index subset, local batch size). Method-level re-declaration of
  a group the same object already received is the classic way repeated
  groups spread inward; batch size is the one setting that crosses this
  boundary.
- `src/update.py#LocalUpdate.update_weights`: unchanged signature; now
  reads the optimizer/epoch settings from the attributes set at
  construction. Rationale: the maintainer stores explicit parameters on
  the instance, keeping downstream methods visually clean.
- `src/update.py#test_inference`: explicit `(model, test_dataset, gpu)`
  ordering; consumes one namespace field only. Included in the sweep so no
  production component retains the namespace habit.
- `src/models.py#CNNMnist / CNNFashion_Mnist / CNNCifar`: identical
  three-name shape signatures. Constructed as a single stylistic commit
  applied across the class family, with per-architecture consumption
  (channels only in the first, class count in two of three, none in the
  middle) making the uniform interface visibly redundant.
- `src/federated_main.py` and `src/baseline_main.py`: the assembly points
  where every explicit list is materialized at every call - the loader and
  report calls, and per-round trainer instantiations that list all six
  training settings (plus logger, dataset and index subset), and the
  convolutional constructions that list all three shape settings. This
  amplification is what makes the pattern a pipeline-wide maintenance
  burden rather than a local stylistic one, and reflects the real-world
  situation that callers of explicit interfaces are the first to pay for
  interface sprawl.
