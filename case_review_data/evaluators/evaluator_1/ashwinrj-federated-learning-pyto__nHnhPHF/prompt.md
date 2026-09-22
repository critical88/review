# Experiment settings travel as long repeated parameter lists

## What we are seeing

This project runs two experiments over the same building blocks: a federated
simulation and a plain centralized baseline. Both share a dataset-loading step
(that also partitions the data over the simulated users), a per-user local
trainer that slices its assigned records into train/validation/test loaders and
runs local optimization epochs, a shared optimizer construction, a startup
report of the run configuration, the model architectures, and a final
held-out evaluation pass.

These components used to receive the entire parsed command-line namespace and
pick out what they needed. Over a series of small maintenance changes we moved
them towards explicit per-component inputs: the loaders now name the data
directory and the dataset facts they need, the local trainer takes its own
training knobs, each convolutional architecture names the shape settings it
uses, and the optimizer construction was factored out so both experiments
build theirs the same way.

The move is half-finished, and it shows:

- The same groups of setting names are now declared over and over in
  different signatures. The dataset facts (name, user count, the IID and
  unequal flags) and the local-training facts (batch size, local epochs,
  learning rate, optimizer choice, verbosity, device) each appear as parallel
  individual parameters in several places at once - including a startup
  helper that only prints them, and a loader-slicing step that re-declares a
  subset of its own trainer's parameters.
- All three convolutional architectures spell out the same channel / filter /
  class parameter names even though most of the three use almost none of them.
- Both experiment scripts assemble these long lists by hand at every call,
  inside and outside the training loop. Adding one new knob means touching
  every one of these lists, and a spot missed silently defaults or breaks at
  run time.

## What we would like

Please finish the untangling, in the direction we were already heading:

- Identify every place on the training side where the same related settings
  travel together as parallel individual parameters: loading and
  partitioning, the startup report, the local trainer and its loader
  slicing, the shared optimizer construction, the model architectures, and
  both experiment entry points.
- Introduce cohesive grouping objects for these related settings - one per
  role, not a single bag for everything - and route each component and call
  site through the grouping its consumers read from, so that each boundary
  declares the group once instead of re-listing its members.
- Groupings should carry the settings their role actually consumes.
  Forwarding the whole parsed command-line namespace along is the style we
  are replacing, and a component that needs one of its settings in several
  steps should read it from one place rather than keeping extra copies.

## What must stay true

- Behavior must not change: dataset selection and directory (including the
  current behaviour where dataset names other than mnist/cifar load the
  fashion-MNIST directory and plain MNIST data), the partitioning algorithms
  and their results, the 80/10/10 local split with its batched shuffled
  train loader, NLL-based training, the model geometries, and the evaluation
  pass over the held-out split all behave exactly as today.
- The command line surface stays as-is: same option names, defaults and
  flags, and both experiments run the same way as today.
- The startup report keeps printing the same facts it does now (dataset,
  model, optimizer, learning rate, global rounds, IID/Non-IID status, user
  count, fraction of users, local batch size, local epochs).
- Do not rework the partitioning algorithms or the model internals; this
  request is only about how settings are handed between components.
