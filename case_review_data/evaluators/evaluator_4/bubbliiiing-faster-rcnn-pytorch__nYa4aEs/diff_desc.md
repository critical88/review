# Injection design record — faster-rcnn-pytorch @ d81ba09

## Maintenance motivation being modeled

The repository is a popular single-GPU Faster R-CNN implementation. Its
authoring style is heavily decomposed: per-batch target construction, proposal
filtering, box encoding/decoding, image preprocessing and evaluation each live
behind small, named units, and every hot loop is a short orchestrator that
calls them. That style makes the pipelines easy to follow, but it also means a
training step crosses a lot of Python call boundaries per image.

The change models a familiar "speed pass" that maintainers of
performance-sensitive Python projects actually make: a profiling session blames
Python function-call and attribute-lookup overhead on the per-image paths
(training target construction, proposal filtering, evaluation preprocessing,
dataset sampling), and the response is to stop calling the helpers — copy the
hot bodies directly into the callers, bind repeated attribute lookups to
locals, and keep the numeric behavior exactly as it was so training logs and
mAP numbers do not move. The result reads as ordinary optimization work: it
carries a plausible performance story, keeps working, and leaves the original
helper units in place "because other callers still use them".

## Development evolution being modeled

A sequence of small per-pipeline commits, each inlining a different call chain
one level deeper, is modeled as one diff:

1. the RPN forward stops calling the grid-enumeration helper for the anchor
   tensor and stops calling the proposal-filter unit per image;
2. the trainer's forward stops calling the two target-creator units and starts
   constructing targets and sampled ROI labels itself;
3. the evaluation callback stops calling the shared image helpers and the box
   decode unit for every evaluated image;
4. the dataset accessor stops calling the shared loading/augmentation method.

Each step is a plausible standalone "micro-optimize this loop" change; the
modeled end state is what such a sequence looks like when nobody pushes back.

## Overall design

Four hot paths across four production files and four classes receive the
treatment. In every location the same moves are applied, which is what makes
the outcome read as one coherent work pattern rather than as separate hacks:

- the body of the helper(s) being called on the hot path is copied into the
  caller, adapted so it runs against the caller's local variables;
- repeated `self.<component>` / `self.<param>` lookups are hoisted to local
  aliases before the embedded body (presented, and genuinely usable, as
  avoiding per-iteration attribute lookups);
- fragment-local names get short site-specific prefixes so the embedded chains
  can coexist with the caller's own bookkeeping (`ious_*`, `sample_*`,
  `roi_base_*`, per-image `*_t` names), so the copies diverge from their
  originals at the token level;
- guards that the callee used to own (empty-ground-truth handling, empty-tensor
  decode) are folded into the caller as explicit if/else blocks;
- the original helper units are left in the tree, untouched and still correct,
  because prediction and other entry points continue to call them, so the
  duplicated knowledge is silently second-sourced;
- comment blocks are rewritten in the caller to describe the embedded
  computation as if it belonged there (Chinese, matching the file style),
  with a one-line "hot path, avoid call overhead" justification per site.

No signature, constant, threshold, tensor shape or draw order is changed at
any site, so training, evaluation and data loading remain numerically
identical.

## Per-site explanations

### 1. `nets/frcnn_training.py` — training step (per-batch target construction)

The training step previously delegated per batch to two creator units: one
turning anchors+ground truth into RPN regression/label targets, one sampling
proposals into ROI classifier targets. Both run for every batch, and both sit
on top of a small hierarchy (label creation, IoU matching, box-to-delta
encoding), which made them the most attractive target for the "stop calling
helpers" pass.

What changed: the step now constructs RPN targets inline — label
initialization to ignore, the broadcasted IoU overlap arithmetic (tl/br
corners and areas) under `ious_*` names, the argmax match assignments, the
threshold labelling and the positive/neg subsampling draws — and computes the
regression deltas with a second inline copy of the box-to-delta body under
`src_*`/`dst_*`-style names. It then concatenates proposals with ground truth
and repeats a second IoU/sampling copy with `sample_*` names to build the ROI
classifier targets, including the empty-ground-truth branch as an explicit
if/else and the +1 background offset. Creator configuration is read through a
local creator alias.

Why this shape: this is the only site where a *chain* of sub-units (creator →
label creation → IoU matching/encoding) is absorbed in one go, so the embedded
fragments interleave with real orchestration (loss assembly, list
bookkeeping) instead of forming one contiguous paste block, and the two
copies of the same overlap/encode bodies on one method make the maintenance
hazard concrete: the same knowledge now lives twice *inside this method alone*,
under two different name prefixes.

Production role served: feeds `rpn_loc_loss`, `rpn_cls_loss`, `roi_loc_loss`
and `roi_cls_loss` every training step.

### 2. `nets/rpn.py` — proposal network forward (per-image proposals)

The forward previously called the grid-enumeration helper once per feature map
and the proposal-filter unit once per image.

What changed: the anchor tensor is now built inline (shift axes, meshgrid,
`np.stack`, the base-plus-shift broadcast) with `feat_stride`/`anchor_base`
hoisted to locals, and a per-image block embeds the filter unit's body —
mode branch, tensor conversion, region decode expanded from the decode helper
(auxiliary zero-source guard added as an if/else), edge clamping, min-size
filter scaled per image, score sorting with pre-NMS truncation, NMS, and the
random padding draw — with per-image `loc/score/roi → *_t` locals and the
filter unit read through a `creator` alias.

Why this shape: the proposal loop is the most profiler-visible path in the
model ("this runs 12000 boxes per image"), so it is where a maintainer would
plausibly start; and it gives the pattern a second, structurally different
presence — the absorbed bodies live inside a loop in a `torch`/`numpy` mixed
boundary region, not at module scope.

Production role served: produces `rois` and `roi_indices` for the classifier
head in both training and prediction modes, plus the anchor tensor passed
downstream for loss assignment.

### 3. `utils/callbacks.py` — evaluation callback (per-image detection txt)

`get_map_txt` previously used the shared image-size/convert/resize helpers,
handed the network outputs to the box-decode unit, and then rescaled boxes to
image coordinates.

What changed: the short-edge sizing computation, the color-conversion
protection, the resize and the input normalization are inline; the decode
unit's body is copied in (rois viewed, std/num-classes read through a
`bbox_util` alias, region decode expanded with its guard as an if/else,
per-class confidence threshold, NMS, label stacking) followed by the original
correct-to-image-space arithmetic inlined into the same loop, with
`input_shape`/`image_shape` rebound to arrays inside the fragment. Two imports
the embedded copies need are added at module top, as the perf narrative
requires.

Why this shape: evaluation runs the decode per validation image, so the "call
overhead in a loop" story holds without further invention; and this site is
visibility-familiar — it sits directly beside the prediction code path in the
packages' public mAP flow, so a future reader comparing the two would find the
same knowledge maintained in three places (here, the prediction method, and
the still-existing helper).

Production role served: writes `detection-results/*.txt` files consumed by the
mAP computation every eval epoch.

### 4. `utils/dataloader.py` — dataset item accessor (per-step sampling)

`__getitem__` previously delegated to the class's own loading/augmentation
method, which owns the shared preprocessing contract used across the
project's train/val paths.

What changed: the accessor now binds the helper's default parameters locally
(`jitter/hue/sat/val`, which the helper also declares as adaptive knobs),
opens the image and inlines the color-conversion protection as an explicit
pass/else (an infra-idiom change rather than a plain rewrite of the call),
inlines the non-random letterbox branch including box adjustment, and keeps a
duplicated assembly tail in both branches (box tensor padding, slicing of
boxes and labels) instead of the helper's single tail. The train branch keeps
using the dataset's own random helpers, so the augmentation remains identical.

Why this shape: data loading is the other "per-step loop" every profiling
write-up blames, so the pass would not stop before it; the site also
demonstrates the accessor absorbing its *own class's* method — the
closest-possible delegation edge — which is what makes later readers unsure
where the shared preprocessing contract actually lives.

Production role served: produces the (image, boxes, labels) items consumed by
the collate function for every training/val step.

## What the four sites share

Every site is a hot loop participant with a plausible per-iteration cost
story; every embedded fragment is renamed and adapted rather than pasted
verbatim; every site leaves the original units in place so the codebase now
maintains the same knowledge twice; and every fragment, despite its renames,
still re-derives observable behavior bit-for-bit (same draws, same order,
same thresholds) — which has so far masked the maintenance question entirely.
