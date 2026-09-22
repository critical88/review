# Re-decompose the network construction code across the pruning pipelines

While porting a cifar experiment to the newest snapshot I ran into something that
has been slowing us down for a while: the architecture definitions in this
repository have drifted into a shape where each network builds itself inside one
long entry body. Reading the model files under the cifar and imagenet pipeline
tree, the code that picks layer widths from a pruning configuration, the code
that chains residual or dense stages together (including shortcut wiring), and
the code that hand-initializes freshly created parameters with raw tensor
mutations all sit interleaved in the same constructor or factory body. Several
of these used to read as a short entry point delegating to one or two focused
helpers, and those seams are gone; what remains is a wall of nested loops where
changing an initialization rule or adding a width layout means re-reading the
whole construction of every affected network.

Please restore a sane decomposition for the network-setup code in the model
definition modules of the pipelines (the per-method pruning trees under cifar/
and imagenet/, not the training drivers). Investigate the architecture families
there broadly - the VGG-style feature-stack models, the residual-stage models,
the dense-block models, and the slimming factory variant that also handles
checkpoint loading - rather than stopping at the first file you happen to open.
Re-extract the inlined construction logic into well-scoped methods or functions
at whatever abstraction level you find most natural, so that staging/layout
concerns and parameter initialization no longer have to be read, reviewed, or
edited as one fused block.

This is a restructuring task, not a behavior change, and the safety bar is high:

- every network must produce bit-identical construction results: the same
  module attribute names in the same registration order (the pruning scripts
  navigate modules by name), the same parameter counts and state-dict layout,
  and the same random-number consumption order so seeded runs are unchanged;
- per-layer channel widths driven by the pruning configurations must stay
  exactly as they are, including the slimming variant's classifier widths and
  its channel-selection hookups;
- the per-family initialization conventions (including the gain values used
  for batch-normalized layers and which layers get bias or gain resets) must be
  preserved, as must keyword-driven gating of initialization and the optional
  pretrained-checkpoint path;
- public constructor signatures that the training and pruning drivers rely on
  must keep accepting the same arguments.

The repository's pytest suite must pass unchanged - treat the tests as the
executable statement of preserved behavior and do not edit them. No new external
dependencies.
