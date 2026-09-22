# Injection design record — segmentation backend seam

## Maintenance motivation

The repository maintains two runtime flavours of the same product: a
`packages/node` library that decodes and encodes with sharp and executes models
through onnxruntime-node, and a `packages/web` library that does the same with
canvas bitmaps and onnxruntime-web behind a worker. Both flavours historically
reached for a loose set of workspace helpers directly (`utils`, `codecs`,
`resource`, `onnx`, `url`), so the pipeline stages were wired together by
module imports and nobody could say, in one place, what a segmentation backend
is responsible for.

A recurring maintenance request is to make the runtime replaceable: support a
remote inference backend, a GPU-backed variant, or a pure-wasm flavour without
touching pipeline code. The natural evolution for that request in this
codebase's style is to introduce a backend contract per package, put a shared
plumbing class under it for the parts that do not depend on the concrete codec
or runtime, and have pipeline code talk to a single default backend instance.
That is the evolution modeled here — land the "one backend seam" scaffolding
quickly enough to ship, before pulling the seam apart along capability lines.

## Overall design

Each package receives a small `backend/` module:

- a `types.ts` declaring the backend contract every flavour has to implement;
- a `base.ts` with an abstract plumbing class that satisfies the package-neutral
  parts of the contract by delegating to the existing workspace helpers, leaving
  only codec and runtime operations abstract;
- a leaf class (`sharp.ts` in node, `canvas.ts` in web) implementing the
  abstract operations on top of sharp / canvas and the existing onnx helpers;
- an `index.ts` exporting the contract types and a package-wide `defaultBackend`
  instance typed against the contract.

The pipeline stages (`inference.ts` in both packages, `index.ts` in node,
`api/v1.ts` in web) are rewired to obtain backend operations through
`defaultBackend` instead of importing the workspace helpers directly. Nothing
about helper behavior changes: every delegated call lands on the same helper
function with the same arguments as before.

## Per-location rationale

### `packages/node/src/backend/types.ts` (new)

Declares the Node.js backend contract with the full stage list: image
decoding, image encoding, tensor resizing, HWC→BCHW conversion, proportional
sizing, float-to-uint8 conversion, absolute-URI resolution, fetch-style
responses, chunked-blob loading, resource-object-url building, and onnx session
creation and execution, with a session type alias over `onnxruntime-node`'s
`InferenceSession`. One declaration site is the point of the scaffolding: a
reader can enumerate the whole runtime surface in one file. The contract bundles
stages that historically lived in five modules because a backend was meant to
replace all of them at once; the wide shape follows from that intent rather than
from any single consumer's need.

### `packages/node/src/backend/base.ts` (new)

The abstract plumbing class implements the contract's package-neutral members
by delegating to `utils`, `url`, and `resource` helpers, keeping only decode,
encode, session creation and session execution abstract. This is the shape a
developer picks to avoid duplicating helper delegation in every flavour: the
base once, flavours only for what differs. Consequence that comes with the
shape: any class extending it now carries every member of the contract,
including members that simply re-wrap a helper that some flavour will never
ask for through the backend.

### `packages/node/src/backend/sharp.ts` (new)

The concrete Node.js flavour: sharp-backed decoding and encoding, plus session
creation/execution over `onnx` helpers. Implementing the contract via the
plumbing class means the leaf also re-exposes members it inherited. That is
accepted at this stage because the goal is a compilable default backend with
minimal duplicated code, and the contract is the only seam pipeline code knows.

### `packages/node/src/backend/index.ts` (new)

Module facade: re-exports the contract types and the classes, and defines the
package-wide `defaultBackend` reference typed against the contract. Typing the
instance against the contract — not the concrete class — is what keeps pipeline
files free of sharp/runtime specifics; it also makes the contract the only way
any consumer sees backend capabilities.

### `packages/node/src/inference.ts` (modified)

Model initialization previously called the resource layer and the onnx helper
directly; inference previously called tensor helpers and the onnx helper
directly. Both stages now route those operations through the backend reference.
This is the placement that makes the seam load-bearing: initialization needs
resource assembly plus session creation, while the inference pass needs tensor
math plus session execution — each stage keeps addressing the same
single-purpose object even though it needs only part of what it provides.

### `packages/node/src/index.ts` (modified)

The public pipeline (background removal, foreground removal, segmentation
masking, mask application) previously encoded tensors with the codecs helper and
resized result masks with the tensor helper. Those operations now go through
the backend reference. The public functions are the package's stable API
surface, so the changes are restricted to the operation-wiring lines; input
handling, output format negotiation, progress calls, and mask recombination
stay as they were.

### `packages/web/src/backend/types.ts` (new)

The browser twin of the contract, adapted for the web runtime: no fetch-style
response member, an extra `bitmapToData` member for drawing an `ImageBitmap`
into an `ImageData`, a proportional-resize flag on the tensor-resize member,
and an execution member that takes the config. Session identity is opaque
(`unknown`) because the web runtime hands the session to a worker — the
contract treats the worker boundary as an implementation detail, which is also
why the contract has no member exposing it.

### `packages/web/src/backend/base.ts` (new, plus `canvas.ts` and `backend/index.ts`)

Same plumbing-first shape as the node package: the abstract class delegates
tensor math, bitmap drawing, and resource assembly to the existing helpers; the
canvas leaf implements decode/encode and the onnx session operations; the index
defines the default backend typed against the contract. The browser package
deliberately repeats the node shape rather than sharing the module: the two
packages bundle separately (`@imgly/background-removal-node` /
`-web`), and a shared contract would drag one runtime's concerns, and its
dependency graph, into the other.

### `packages/web/src/inference.ts` and `packages/web/src/api/v1.ts` (modified)

Same rewiring as in the node package, on the browser execution paths: the
pipeline stages request blobs, sessions, tensor math, and session execution
through the backend reference; the versioned public API encodes result tensors
and resizes masks through it. The inference session object stays a
`{base: unknown}` handle across the worker boundary, so initialization and
execution reference it exactly as before.

## Production role summary

The seam converts an implicit, import-wired pipeline into an explicit,
contract-wired one in both runtimes. The abstract plumbing keeps helper
delegation in one place; the leaf classes keep runtime differences in one place;
the contract makes the backend surface addressable as a whole. All existing
workspace helpers remain in place and their behavior is untouched — the seam
delegates rather than reimplements.
