# Injection design record — BIO readiness support

## Motivation

LibreSSL's BIO layer is the one abstraction every I/O wrapper in the tree goes
through: the `nc(1)` and `ocspcheck(1)` applications, the `openssl(1)` app,
and libtls all drive sockets, files, memory buffers and filter chains through
it. A recurring request from users of event-driven programs is a portable way
to use a BIO non-blocking: switch the underlying resource into non-blocking
mode, then park until the BIO can actually be read or written, with a
millisecond bound, without the caller having to know whether it wraps a
socket, a descriptor or something else. On OpenBSD that maps to `poll(2)`, and
the portable tree has to provide the same face on every platform it supports.

The work item this change models is that feature request landing as one
focused library commit: a small non-blocking readiness API on BIO, implemented
across the method layer.

## The evolution being modeled

The commit takes the path of least resistance that a maintainer under review
pressure plausibly takes: it grows the one structure every BIO implementation
already instantiates. The internal method struct behind `BIO_METHOD` gains
three handler slots next to the classic operations, the new public functions in
the library core dispatch through them unconditionally, every method table in
the tree must then fill them (the compiler forces it), and the public accessor
surface grows a new pair of setters/getters so the one runtime-built method can
install them too. No capability probing, no opt-in: being a BIO now includes
answering readiness. Recompiling the whole tree hides the cost: the change is
additive at the source level and reshapes no visible behavior of the classic
operations, so programs built against the library go on running exactly as
before.

This is a recognizable way for an abstraction to decay under a schedule: correct
at every step, awkward as a whole.

## Where the change lives in the tree

This repository does not track its C sources; `./update.sh` materializes
`crypto/`, `ssl/`, `tls/` and `include/` from the upstream snapshot and then
applies the local patch series under `patches/` in filename order. Local
library deltas ship as patch files in that series - that is the production
idiom of this tree and the only way a change like this reaches users of the
portable build. The diff therefore introduces one new tracked artifact,
`patches/bio_readiness_hooks.patch`, which carries the whole library commit
described below. The materialized C files listed in the following sections are
the content of that patch; they are the tree a user of the portable build
actually compiles.

## Design

The feature has four layers:

1. **Shared contract** — the method struct behind `BIO_METHOD`
   (`crypto/bio/bio_local.h`) declares three new handler slots: a
   non-blocking-mode toggle and two poll-based wait hooks, together with the
   small comment that explains the arrangement ("every method table fills all
   three slots; the waiting wrappers in bio_lib.c need no per-method capability
   probing"). A `poll(2)` helper shared by descriptor-backed methods and two
   event-interest macros live next to it.

2. **Public API** — `include/openssl/bio.h` declares the three application
   entry points (toggle plus two waits, one of them replacing the previous
   `BIO_set_nbio` macro with a real function) and, under the method-construction
   accessors, six new getter/setter prototypes for the slots. The header
   comment frames the trio as a universal BIO capability.

3. **Core dispatch** — `crypto/bio/bio_lib.c` implements the three public
   functions as unconditional indirect calls through the slots, plus the
   descriptor-poll helper they rely on; `crypto/bio/bio_meth.c` grows the six
   accessors that read and write the slots on a method.

4. **Implementations** — every production method table in the tree fills the
   three slots; the libtls callback method installs them through the new
   accessors at construction time.

## Per-cluster rationale

### Shared contract and public API

These sites are where the feature is *declared*, so they came first: the slots
must exist on the struct before any table can fill them, the public names must
exist in the header before applications use them, and the construction API must
expose the slots before a runtime-built method can install them. Their
production role is the contract every other site in this design record depends
on.

### Kernel-descriptor families — `bss_sock.c`, `bss_fd.c`, `bss_dgram.c`, `bss_conn.c`, `bss_acpt.c`

These are the types the feature was actually requested for: sockets, raw
descriptors and datagram sockets. Their hooks turn the toggle into the
existing platform non-blocking switch and turn the waits into a `poll(2)` on
the descriptor with the caller's millisecond bound. The connectivity-guarded
types additionally check session state before polling. Each family already
had a private ctrl routine; the hooks follow that file's local conventions.
Their production role is the genuine, resource-backed implementation of the
capability.

### Memory-buffer families — `bss_mem.c`, `bss_null.c`, `bss_file.c`, `bss_bio.c`

These types wrap resources that cannot block: an in-memory buffer, the null
sink, stdio streams (no readiness signal exists for `FILE *`), and the
pre-connected BIO pair. Because the slots are now part of the contract, these
tables must still supply them, so the commit invents plausible answers: a
buffer reports pending bytes as readable and always writable; the null sink
answers yes to everything; stdio answers no; the pair inspects the peer's
buffer occupancy and its own capacity. These eager stubs are the honest
consequence of the declared contract rather than a bug - the resource
genuinely never blocks, so the answers describe that.

### Relay/filter families — `bf_buff.c`, `bf_null.c`, `bf_nbio.c`, `crypto/evp/bio_enc.c`, `crypto/evp/bio_b64.c`, `crypto/evp/bio_md.c`, `crypto/asn1/bio_asn1.c`

Filters have no resource of their own; their whole job is to forward to the
next BIO in the chain. Their hooks consult whatever intermediate state each
filter privately buffers (the line buffer's input/output buffers, the
cipher's output buffer, the base64 filter's decoded output) and otherwise
propagate the call to `b->next_bio`. Their inclusion is mechanical - the
contract leaves them no way to stay out - and the per-family differences come
from what each filter happens to buffer.

### SSL record layer — `ssl/bio_ssl.c`

The SSL BIO answers readiness from the protocol layer rather than only from
the wrapped socket: pending decrypted records count as readable, and the
toggle/waits forward to the configured read and write BIOs of the session.
This site exists because the table layout in this file sat close to its
column limit already, so the three designators were compressed into fewer
physical lines than elsewhere - a small formatting variation within the
house style.

### Runtime-built method — `tls/tls_bio_cb.c`

libtls builds its callback BIO dynamically with the public construction API
at first use; it cannot use a static initializer at all. This file therefore
shows the third shape the feature takes: three of the new setter accessors
called right after the classic setters in the method-completion routine. Its
production role is to demonstrate the dynamic-method obligation - even a
method composed at runtime must answer readiness, which is what made the new
accessors part of the public construction surface.

## Deliberate structural variation

The design intentionally avoids making the sites uniform:

- one declaration site (method struct), one dispatch site (public wrappers in
  the library core), one construction-API surface (the accessors);
- seventeen static table initializers spread over four directories
  (`crypto/bio`, `crypto/evp`, `crypto/asn1`, `ssl`), each filling the slots
  with that file's own naming and ordering conventions, plus the one
  runtime-constructed method in `tls`;
- four distinct semantic answers (kernel polling, deterministic stubbing,
  own-state inspection, chain forwarding), chosen per resource kind, and one
  protocol-level answer in the SSL BIO;
- the pair-BIO file places its static initializers before the hook
  definitions and therefore needs short forward declarations for its hooks -
  the only file in the set that does.

## Production role of the series entry

`patches/bio_readiness_hooks.patch` is the shipping artifact: the portable
repo compiles what `./update.sh` produces, and this file is what adds the
readiness support to the vendored sources at materialization time. It sits in
the series after the existing hygiene and portability patches, and it is
intended to be reviewed and iterated as a patch file, like every other local
delta in this tree. No new regression program is carried by this series entry:
it deliberately contains source changes only, matching how the other library
patches in the series are carried today.
