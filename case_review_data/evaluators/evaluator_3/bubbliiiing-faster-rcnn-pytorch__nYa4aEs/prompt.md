# Restore the helper structure on the hot paths (speed-pass cleanup)

Some time ago this project got a "speed pass". Whoever did it was convinced
that Python function-call and attribute-lookup overhead was hurting the
per-image work, so on a few of the hottest paths they stopped calling the
small helper units this codebase is built around and copied the bodies of
those units straight into the callers, binding a couple of attribute lookups
to local aliases on the way. The original helper units were left in the tree,
so the same knowledge is now maintained in more than one place, under
different local names at each site.

To be fair, it behaves: losses, detections and dataloader output are what
they always were, which is probably why nobody reverted it. But the methods it
touched have turned into wall-of-code bodies that re-derive things the
project already owns — anchor/region geometry, IoU overlap, the
boxes-to-deltas and deltas-to-boxes adjustments, positive/negative target
assignment and sampling, grayscale-to-RGB protection, resize/letterbox
sizing, coordinate scaling. A couple of sites even hold two adapted copies of
the same math in one body, with name prefixes that differ per copy. New
contributors get lost in these methods, and every fix now has to be applied
in several places at once or the copies drift.

I run into this in the areas that were profiled hardest — the training step
where the losses are computed, the proposal path, the per-image work the
evaluation callback does, and dataset sampling. I did not audit every file,
so if the same pattern shows up somewhere else, consider it part of the same
problem.

Please restore the decomposed structure: identify where hot-path methods embed
copies of logic that belongs behind well-scoped methods, and move each
embedded responsibility back behind a method at the right abstraction level,
so these paths delegate again instead of re-deriving inline. Keep the
decomposition honest — the goal is one maintained copy of each piece of
knowledge, not a rename of the embedded fragments.

Hard requirements:

- **No behavior change.** Identical inputs and identical seeds must produce
  identical losses, predictions and dataloader output. In particular the
  order of every random draw (numpy subsampling and padding draws, dataset
  augmentation draws) must be preserved — seeded runs must reproduce exactly.
- **Keep the public shape stable:** model/trainer constructor arguments and
  forward signatures, the dataset item contract consumed by the collate
  function (CHW float image, boxes, labels), and the detection-results txt
  output used for mAP.
- **Do not strand or fork the shared units:** the existing helper methods
  stay usable by all of their callers (training, prediction, evaluation
  entry points alike), with unchanged meaning. If you re-derive a helper's
  responsibility into a new unit, there must not be two competing copies
  left behind.
- The project's tests keep passing.
