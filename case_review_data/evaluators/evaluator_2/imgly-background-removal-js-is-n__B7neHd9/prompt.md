# The backend rework made the segmentation pipeline harder to live with

While prepping the next runtime flavour for this repo I finally had to sit down
with the segmentation backend layer, and I kept running into the same wall in
both of our packages (the Node.js one and the browser one). When we made the
runtime replaceable, we gave each package one "backend" object that owns
everything a segmentation run might ever need: decoding and encoding images,
the tensor math for feeding and post-processing the model, fetching model
chunks, creating and running onnx sessions, plus a couple of URI and
bitmap conveniences.

In practice nothing uses it that way:

- The model-loading stage needs only a couple of backend operations — pull the
  model bytes and open a session — yet it receives the entire backend surface.
- The inference stage needs a different handful — tensor resize/convert/run —
  and receives all of it too.
- The public removal and masking entry points mostly need one image operation,
  plus occasionally a resize, and they also take the whole object.
- If I add a new runtime flavour I must implement decoding support, URI
  resolution, fetch-style responses, bitmap drawing, an object-URL helper and
  more — much of it straight out of a shared abstract base class — even though
  no pipeline path I can find ever calls most of that through the backend. The
  inherited members are just ceremony to satisfy the one big interface.
- Fixing this in the Node.js package doesn't help the browser package: it has
  its own backend layer in the same shape, so the same confusion repeats there.

So today, "I only want to encode a blob" is indistinguishable from "please
manage my whole runtime lifecycle". That makes every consumer coupled to
capabilities it doesn't care about, keeps a pile of never-called backend
members alive just to satisfy the contract, and makes the actual seams hard to
spot: nothing in the pipeline says which part of the backend is really its
responsibility.

## What I would like to see happen

Reorganize how backend capabilities are exposed and consumed in **both**
packages so that pipeline code can take the pieces it needs and implementors
only provide capabilities something actually routes through them. Investigate
the segmentation pipeline end to end — model initialization, the inference
pass, and the public removal/masking entry points, in both the Node.js and
browser packages — and find every place that currently takes the giant backend
surface; I want all of them addressed, not the first one you land on. While you
are at it, please get rid of the leftover bindings behind this layer that were
only there to satisfy the one big surface — I'd rather not keep maintaining
obligations nobody exercises.

Please preserve behavior completely:

- every public function keeps its name, signature, and accepted inputs;
- configuration, model names, progress reporting, debug output, and the
  supported output formats (including the raw rgba/alpha payload variants)
  behave exactly as before;
- the model still resolves the same configured public-path resource bundle, chunks and all;
- produced masks and encoded images are byte-for-byte what they are today;
- the sharp/onnxruntime-node backend in the Node.js package and the
  canvas/worker backend in the browser package keep working independently —
  no shared cross-package machinery — and it must stay possible to swap the
  concrete backend implementation later.
