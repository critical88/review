# Design record for the case diff

This is the authored design rationale for the diff (`smell.diff`) that
transitions the repository from its pinned state to the case state. It
records why a maintainer would plausibly produce these edits, what shape
they take, and why each location was chosen. It does not draw conclusions
about quality or classify the result; that is for later review.

## Maintenance motivation

This repository is the companion code of a hands-on Rust course. The KV
service it uses as a running example is reworked generation by generation:
each chapter keeps the service's behavior and upgrades one dimension of its
engineering — from a threaded KV, to a secure TLS KV, to mutual TLS, to
multiplexed sessions, and finally to a fully configuration-driven service.

In the TLS chapters the author's recurring teaching problem is: rustls and
tokio-rustls wrap a lot of knowledge inside `ServerConfig`, `ClientConfig`,
`TlsAcceptor` and `TlsConnector`, and each generation's lesson wants to show
learners *what actually goes into a TLS session*. Experienced Rust
maintainers will recognize the drift this produces: keep the wrappers thin,
move the mechanics into plain, greppable functions, make every step of
"identity in, session out, stream converted" explicit and separately
readable, and carry the raw material from layer to layer unchanged so each
layer can be explained on its own.

These facts motivate the diff: it is the natural outcome of a maintainer
flattening wrapper types across a multi-generation codebase to make the TLS
layer explicit lesson by lesson, while neither generation's observable
service behavior changes.

## The development evolution being modeled

Each generation of the KV service previously owned two small session
holder types of its own: one that holds the built server-side session
configuration and hands accepted TCP streams over into TLS streams, and one
that holds the built client-side configuration plus the target server name
and converts outgoing TCP streams. They were thin, generation-local,
practically free to keep — which is also why flattening them felt harmless:
each generation's public behavior stays identical.

The modeled evolution removes those holder types and rewrites the layer as
free functions, generation by generation, so that every step of the TLS
setup is a function the course can print on one slide. As generations accrete
(plain TLS, then mutual TLS, then multiplexing, then configuration
bootstrap), the free-function layer sprawls along the same grooves: every
new layer that needs the material simply adds the same parameters to its own
signature. That is exactly how parameter groups come to be handed around
everywhere in real codebases: not in one designed decision, but in a series
of locally reasonable additions across a file, then a module, then four
sibling generations that copy the previous chapter's pattern.

## Overall design of the diff

The material that the TLS layer needs comes in two bounded bundles, one per
side of a session:

- The server side needs its certificate, its private key, and the client-CA
  that switches mutual TLS on when provided. Adding anything to the server
  identity (say, a key password or a second CA) means touching every
  function that touches this bundle.
- The client side needs its optional identity pair (own certificate and
  key) plus the server-CA to verify the server with, when the system roots
  should not be relied on.

The diff threads both bundles, parameter by parameter, through every free
function of the four TLS-bearing generations' network layers, through the
two binaries' connection paths, through the re-export surface of each
generation's network module, and — in the configuration-driven generation —
through the bootstrap that reads the material out of the config structure
and hands it down into the per-connection handler. Where ownership must
cross an `async` task boundary (the config-driven generation's accepted
connection), the bundle's items widen from `&str`/`Option<&str>` references
into `Arc<str>`/`Option<Arc<str>>` shared strings, cloned per connection.

## What changed, location by location

### 1. `37_kv` — the first TLS generation (server-side)
`src/network/tls.rs`, `src/server.rs`, `src/client.rs`, `src/network/mod.rs`

kv3 is where TLS enters the course. The lesson goal here is to show the
anatomy of a rustls server: parsing a PEM certificate and key, deciding
between no-client-auth and an authenticated-client policy, installing the
server certificate, and negotiating the K/V ALPN protocol. To make that
anatomy visible, the holder type is dissolved into three plain functions
per side: one that builds the session configuration from raw PEM strings,
one that adapts that configuration into a stream-converting accept step,
and one that performs the accept on a concrete TCP stream inside the
server's connection loop. The `kvs` binary — the generation students
actually run — now threads the three server-material parameters into its
accept loop directly, and the `kvc` binary passes its client-side material
(server CA and optional identity) into the connect step of the client path.
The `network` module's re-export line is updated so the free functions are
the generation's visible surface the lessons refer to.

Role in production: this cluster is the reference anatomy every later
generation copies; it is also the most direct read of "identity as
arguments" in the whole diff.

### 2. `41_kv` — the mutual-TLS generation
`src/network/tls.rs`, `src/server.rs`, `src/client.rs`, `src/network/mod.rs`

kv4 repeats the same free-function layer with one conceptual addition: the
client identity pair becomes optional, and a client-CA on the server side
switches client-certificate verification on. The evolution being modeled is
the chapter adding mutual trust to the service. The implementation keeps
the same function families and parameter names as kv3 so the two chapters
stay comparable line by line; the identity pair is threaded as two separate
optional parameters wherever the client path needs it. The binaries' wiring
mirrors kv3's.

Role in production: shows the bundle growing a new member (the identity
pair supporting `Some`/`None` per call) without any of the surrounding
signatures learning about the concept as a whole.

### 3. `42_kv` — the multiplexed generation
`src/network/tls.rs`, `src/server.rs`, `src/client.rs`,
`src/network/multiplex.rs`, `src/network/mod.rs`

kv5 puts yamux session multiplexing on top of the TLS layer. The lesson
focus shifts up the stack, so the TLS step is kept as the same explicit
free functions, now reused under the multiplexer fixtures: the
chapter-grade test fixtures inside `multiplex.rs` accept the server
material as bare `&'static str` parameters (they hold the fixture
certificates) and pass them to the same accept function the production path
uses. The production server and client binaries behave like kv3/kv4's.

Role in production: carries the material into a second, test-shaped
consumer inside the same crate, showing how the group surfaces wherever the
handshake is set up rather than belonging to one construction path.

### 4. `46_kv` — the configuration-driven generation
`src/lib.rs`, `src/network/tls.rs`, `src/network/multiplex.rs`,
`src/network/mod.rs`

kv6 is the last generation: the service becomes configuration-driven
(a TLS section of the config carries certificate, key, CA, identity, and
the DNS name to trust), the entry points become library functions rather
than binaries, and every connection is served inside a spawned task. This
cluster is the most evolved form of the diff, chosen because it shows the
widest spread of the same material through one crate:

- `src/network/tls.rs` keeps the same six free functions the earlier
  generations use (build/accept-or-connect families on both sides) so the
  final chapter's comparison with the earlier ones stays readable; the
  functions keep their tracing spans.
- `src/lib.rs`'s `start_server_with_config` destructures the TLS section of
  the configuration and hands the items into a new `start_tls_server`,
  which forwards them into `serve_tls_connection`; that per-connection
  function must hand each accepted connection its material inside the
  spawned task, so the parameters widen into `Arc<str>` shared strings and
  are cloned for every connection. The client side is symmetric:
  `start_client_with_config` splits the config into name, identity pair,
  and CA, and a `start_tls_client` function takes over the connect step.
- The multiplex fixtures inside `src/network/multiplex.rs` take the server
  material the same way kv5's do.
- The module re-exports list the free functions and helpers of the
  module-level network/mod.rs`.

Role in production: the config-deconstruction, bootstrap, and
per-connection phases are three genuinely different lifecycle stages the
same material now crosses in one generation — the widest traversal of the
diff, and the one that also shows the group's type widening ownership
(`&str` to `Arc<str>`).

### 5. Re-export surfaces (`src/network/mod.rs` in all four generations)

Each generation's `network/mod.rs` re-exports the TLS helpers the lesson
texts refer to. The lines change from re-exporting the two holder types to
re-exporting the free functions. These sites were chosen deliberately:
they are small, but they decide what the generation's public helper
surface is, so they are where "the next maintainer" first meets the
material-as-parameters shape.

## Structural variation across the locations

Several deliberate variations keep the locations from being copies of each
other, matching how sibling generations really diverge:

- parameter types widen in kv6 (`Arc<str>`, `Option<Arc<str>>`) versus the
  `&str`, `Option<&str>` of earlier generations, because the material must
  survive into spawned per-connection tasks;
- the identity pair is optional on every client-side path but the server
  bundle is always complete;
- kv6's client trust policy differs from the older generations' (a
  configured CA takes the place of the system roots rather than being
  added on top), and the free functions each preserve their own
  generation's policy;
- consumers differ in kind: configuration builders, stream converters, a
  CLI accept loop, a config bootstrap, a per-connection task hand-off, and
  test fixtures.

## Scope boundaries of the authored evolution

Earlier KV generations (the threaded and plain-TCP KVs) have no TLS
identity material, so nothing was changed there. The service command layer,
the protobuf layer, and the generated FFI bindings were left untouched by
this evolution; they describe other concerns of the course material. All
edits stay inside the four TLS-bearing KV generations, which is the
boundary a maintainer of this evolution would naturally respect.
