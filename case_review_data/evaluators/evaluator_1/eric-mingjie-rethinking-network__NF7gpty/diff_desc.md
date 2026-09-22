# Injection design record — deeply inlined network-construction code

This document records how the folded-construction shape in this diff was designed:
the maintenance story it models, the overall design, and the rationale for each
changed location or coherent cluster. It is an auditable design record, not a
task statement.

## Maintenance motivation

This repository is a research codebase for per-method network pruning: each
pipeline keeps its own copy of the architectures it needs, and the same
architecture reappears in several pipeline folders with small local edits. The
natural maintenance pressure on these files is initialization and layout
tweaking: researchers adjust gain conventions, bias handling, width-driven
channel layouts, or the stage budget of a pruning configuration, usually while
debugging one pipeline at a time and usually while the constructor itself is the
only thing on screen. Under that pressure the dedicated construction helpers a
family once had become indirection that has to be jumped through; a maintainer
intent on "seeing the whole network while tuning it" merges the helper bodies
into the construction entry point, works there, and moves on. The merged form
then gets copied into a sibling pipeline the next time that pipeline is
refreshed, with small idiomatic drift per copy because each copy was merged by
hand at a different time. The diff models exactly this ordinary, believable
decay: construction entry points that once delegated now contain the
sub-implementations themselves, and the per-copy drift is visible both in loop
idioms and in comment style.

## Normal development evolution being modeled

The pristine snapshot separates two construction responsibilities per
architecture family: a layout/staging helper that turns a pruning configuration
into module structure (`make_layers`-style feature-stack builders,
`_make_layer`-style stage builders that track channel growth and wire
shortcuts, dense-block and transition builders that run growth-rate arithmetic,
and the module-level slimming factory that orchestrates feature construction,
classifier-width recording, initialization gating, and optional checkpoint
loading), and a weight-initialization protocol (`_initialize_weights`-style
methods or inline modules-walks that mutate fresh tensors in place). The
plausible evolution into this diff: during a tuning session the two helper
bodies get absorbed into the entry point so the tuning target is one readable
block; the now-unused helpers are deleted; the same merge gets repeated for a
different pipeline later, but re-typed rather than copied, so each site acquired
its own idiom; and the factory-level slimming variant absorbs its own
orchestration the same way because there the maintainer was tuning the
pretrained-loading path. Alongside the model-file work, the diff carries a
compact regression harness (loader plus pytest suite and packaging metadata)
that pins construction behavior — parameter counts, state-dict layout, channel
widths, gain conventions, forward/backward shapes — so that the accumulated
construction state remains measurable in any checkout.

## Overall design

One structural move is applied consistently across the architecture families and
the pipelines that own them: absorb the layout/staging sub-implementation and
the raw parameter-initialization sub-implementation into the single construction
entry body that calls them, then remove the absorbed helpers. The absorbed code
is adapted to its new home rather than pasted verbatim — variable naming takes
the entry point's context, guard conditions become the entry point's own
conditions, and the assembly idioms differ per site (see "Deliberate structural
variation"). Construction order, module registration order, attribute naming,
initialization draw order, and the gating semantics are held constant at every
site, so the folded form remains behavior-identical to the snapshot it replaces.

## Per-cluster rationale

**Cifar l1-norm-pruning VGG (`cifar/l1-norm-pruning/models/vgg.py`).** The cfg
expansion loop and the batch-norm-aware stack assembly moved into `vgg.__init__`,
followed by the parameter protocol in the same body; the family's helper pair
disappeared. Chosen as the VGG-feature-stack representative of the
prune-from-scratch pipeline: it is the most-tuned cifar model in the repository
(its configuration drives the whole l1-norm study), so a maintainer folding one
model and leaving the result in the tree is most credible exactly here. Its
production role: the primary cifar l1-norm pruning architecture.

**Cifar lottery-ticket l1-norm-pruning VGG
(`cifar/lottery-ticket/l1-norm-pruning/models/vgg.py`).** Same absorption, but
re-typed: the stack grows through `layers += [...]` appends driven by an
enumerating loop instead of list pushes, and the comment voice matches that
pipeline's file history. This models the sibling-pipeline re-merge done from
memory rather than by copy. Production role: the lottery-ticket variant of the
same study, whose runners must keep constructing identical modules.

**Cifar l1-norm-pruning ResNet (`cifar/l1-norm-pruning/models/resnet.py`).** The
stage builder was folded into the entry point as a tuple-driven table of stage
windows over the pruning configuration, keeping the family's
`partial(downsample_basic_block, ...)` shortcut protocol interleaved with the
stage chain, and the inline conv/linear initialization that this file already
carried now sits adjacent to the assembly loops. Selected to carry the
residual-stage shape with the repository's distinctive function-tool shortcut
wiring, which exists only in this family. Production role: conditional-stage
residual architecture for l1-norm and channel-constrained runs.

**Cifar network-slimming DenseNet (`cifar/network-slimming/models/densenet.py`).**
The dense-block and transition builders were absorbed as nested unit/stage
loops that run the growth-rate and compression arithmetic inline, together with
the initialization protocol. Selected because the dense family has the deepest
natural assembly math in the repository (growth arithmetic, transition
budgets, two-class-side bookkeeping around the channel-selection masks), so its
folded body grows the longest and shows the readability cost at its worst, while
bringing the channel-selection slimming pipeline into the changed surface.
Production role: the slimming study's dense architecture with mask-coupled
select layers.

**Imagenet l1-norm-pruning ResNet (`imagenet/l1-norm-pruning/resnet.py`).**
The stage builder folded here as a cursor-walked windowing of the configuration
across four stages, with explicit sequential shortcut construction
(Conv/BN pairs) inlined before the first block of each stage and initialization
kept in the same body. Deliberated different from the cifar l1 ResNet idiom
(tuple tables there, cursor windows here) to model independent maintenance
history between the cifar and imagenet trees while keeping the same structural
relation. Production role: the imagenet-scale l1-norm pruning architecture.

**Imagenet network-slimming VGG factory
(`imagenet/network-slimming/vgg.py`).** The module-level factory absorbed the
make_layers-style expansion of its pruning configuration into conv/BN/ReLU
features, the config-derived classifier-width recording, the keyword-gated
initialization (kaiming-style conv fan-in fills and the batch-norm conventions
the classifier head relies on), and the optional pretrained-checkpoint path.
Selected as the only module-level orchestration site: it shows the middle of a
factory call collapsing inward, not just class constructors. Ordering-sensitive
pieces (recording widths, then disabling delegated initialization, then
constructing the classifier-carrying module, then applying the replacement
protocol, then loading the optional checkpoint) stayed in their original order.
Production role: the slimming entry point used by the imagenet study and its
finetuning follow-ups.

**Cifar weight-level (Lottery Ticket) ResNet
(`cifar/weight-level/models/cifar/resnet.py`).** The fixed-width three-stage
layout and the explicit one-by-one shortcut construction were folded into the
constructor alongside the initialization loop this file already carried. Chosen
to bring the weight-level pipeline into the changed surface with a
representative distinct from l1-norm's cfg-windowed shape: here the stages are
fixed widths rather than configuration windows, so the assembly idiom differs
again. Production role: the weight-level pruning family's residual
architecture.

**Regression harness cluster (`tests/`, `pyproject.toml`).** A compact
per-instance pytest suite with a two-namespace loader and packaging metadata so
the accumulated construction behavior of every pipeline above is pinned and
executable from any fresh checkout of the tree. It documents the status quo of
construction results (counts, layouts, widths, gain conventions, shapes) so
later construction work has a measurable contract; it makes no statement about
code layout beyond what those results require.

## Deliberate structural variation

The folded shapes intentionally do not repeat one textual pattern. Stage tables
are tuple-driven in one file and cursor-walked in another; stacks grow through
list pushes in one copy, `+=` appends in another; stage registration interleaves
with `setattr`-naming in some sites and direct stage locals in others; the
dense site nests unit loops inside stage loops while the VGG sites stay flat;
shortcut wiring appears both as the functional `partial` protocol and as
explicit sequential pairs depending on the family; initialization guards range
from explicit keyword gates to no guard at all, matching what each file's
original own protocol looked like. Comments and naming follow each site's
surrounding file voice. The variation exists because merged-body drift in
research codebases is per-file history, and it also means the changed surface
cannot be described (or undone) by one mechanical text transformation over the
tree.

## Scope boundary of the record

This record describes the changed locations and why each took its shape. It does
not predict how the maintenance story ends, prescribe a preferred
re-decomposition, or state what any particular downstream reader should do with
the tree.
