# Injection design record: explicit context threading in the raw FST layer

Repository: BurntSushi/fst @ 5907b4739793b3d5d7061eaa3f85274e09769d6a
Selection outcome: data_clumps (hard difficulty profile)

## Maintenance motivation

The raw layer of this crate (`src/raw/`) is the performance-critical core: it
decodes nodes straight out of the serialized byte stream during lookups and
serializes builder nodes back into it during construction. Two maintenance
pressures drive the change recorded here, both of which come up in real
streaming-compression projects:

1. **Hot-path decoding wants raw access.** Key lookups walk every byte of a
   key through node decoders. A maintainer tracing lookup latency wants the
   walkers to answer questions about *one node at one address* — "is it
   final?", "which transition matches this byte?", "what value does this
   transition carry?" — without routing every question through the general
   node wrapper. Making the (format version, backing bytes, node address)
   context explicit lets specialized readers make those single-field reads
   directly, and lets later work add readers without repackaging a whole node
   each time.
2. **Registry hygiene while everything is being split up.** The node
   registries deduplicate compiled nodes. Their keys are the pieces of a
   builder node: its final flag, its final output and its transition list.
   While the compile side is being reworked, it is tempting to pass the cached
   payload as individual fields so that both registry implementations and the
   serializer can be exercised with loose values first, and to keep the two
   registry backends consistent as they switch.

## Development evolution being modeled

The record models a period in which the maintainer dissolves the two
convenience structures of the raw layer:

- the `Node<'f>` decode wrapper (version, bytes, state, address, derived
  layout factors), and
- the `BuilderNode` compile payload (final flag, final output, transition
  list)

into their individual values, in several plausible-sized commits:

1. **Raw node helpers.** Single-field readers (`is this node final?`, `give
   me its final output`, `how many transitions?`, ...) appear as free
   functions taking (format version, bytes, address) plus a position or byte,
   each forwarding through the node wrapper for now.
2. **Raw walkers.** The key-walk family on `FstRef` (value lookup,
   membership, key-for-value search and the empty-key case) is rewritten as
   free functions over (version, bytes, root address) with the walking
   address threaded by hand between the helper calls. `FstRef` methods become
   thin adapters pulling the three values out of `meta` and `data`.
3. **Unfolded state readers.** The per-state decoders behind the node
   wrapper stop taking the wrapper and take its fields individually
   instead — first the one-transition families (which need only a subset),
   then the many-transition family, which needs the full context: version,
   bytes, node start, node end, pack sizes and transition count, plus the
   query position/byte. Call-site updates in the node wrapper thread
   `self`-fields through. Some of these readers do not need every piece.
4. **Builder split.** The compile-side node payload (`is_final`,
   `final_output`, `trans`) is threaded as three separate arguments through
   the compile dispatch, the many-transition serializer, and the node the
   registry returns. `Builder::compile` and the `BuilderNode::compile_to`
   indirection are dissolved in the same move: the builder gets three free
   compile steps (`builder_compile_node`, `builder_compile_from`,
   `builder_compile_root`) that take the writer, the registry and the last
   compiled address separately, and the payload pieces per node.
5. **Registry keys as field triples.** Both registries are switched from
   `&BuilderNode` keys to explicit (final flag, final output, transitions)
   keys: hashing takes the fields, the LRU cell cache compares and stores
   them field by field through two small helpers, and the minimal registry
   reassembles a node value to key its map.
6. **Drift.** While touching every signature in this layer, some functions
   grow a version parameter they do not actually read (they exist to keep
   sibling signatures shaped alike during the transition), and naming moves
   toward `version`/`data`/`addr` everywhere.

## Overall design

- Signatures in the raw tree now state, one by one, the values a caller must
  supply: which bytes, which format version, which address (or: which final
  flag, final output and transitions on the compile side).
- The walkers in `src/raw/mod.rs` operate on addresses directly and pull the
  fixed starting values from `meta`/`data` only at their boundary.
- The per-state decoders in `src/raw/node.rs` remain the only encoders/decoders
  of the byte layout, but as functions of loose values rather than the
  wrapper. The node wrapper survives for the stream machinery and reuse in
  tests, now backed by the same decoders.
- The builder and the registries agree on the loose payload shape; the registries
  keep their LRU cells but no longer pass node values across their boundary.

## Location notes

### `src/raw/mod.rs` — walkers and their boundary

`fst_get`, `fst_contains_key`, `fst_get_key_into` and `fst_empty_final_output`
are free walking functions over an explicit (version, data, address) context.
`FstRef::{get, contains_key, get_key_into, empty_final_output}` shrink to
adapter methods that unpack `meta.version`, `data` and `meta.root_addr` once
per call. `get` mirrors the original loop (find-input, transition, output
union); `get_key_into` keeps the original value-difference walk against the
transition list; `contains_key` keeps the address-only variant. These sites
were chosen because they are the only entry points through which key lookups
reach node decoding, and they are the natural place for a raw-access era to
begin. The public `Fst`/`Set`/`Map` and stream surfaces are untouched; they
call the adapters.

### `src/raw/node.rs` — single-field raw readers

The six free helpers (`node_is_final`, `node_final_output`,
`node_transition_count`, `node_transition`, `node_transition_addr`,
`node_find_input`) answer one question each about the node at an address,
consistently taking (version, bytes, address, position-or-byte) and forwarding
to the node wrapper. This mirrors the shape the maintainers chose so that
adding a per-field reader is a one-liner, and it is where walker-side call
sites redirect.

### `src/raw/node.rs` — state decoders

`StateOneTransNext::{input, trans_addr}` and
`StateOneTrans::{input, output, trans_addr}` accept only the values they need
plus the drift arguments that keep their shapes aligned with the
many-transition family (`version` used or unused, `start`/`end`, `sizes`).
`StateAnyTrans::{trans_addr, input, find_input, output}` require the full
context: version, bytes, node start, node end, pack sizes, transition count
and the queried transition/byte, with all layout arithmetic moved onto those
parameter values; `find_input` keeps its original two branches (index table
for large nodes, linear scan otherwise). These functions are the last
arbiters of the on-disk format; the parameter rename (from `node.start` to
`start`, etc.) leads to the current stretched signatures. `Node::transition`,
`Node::transition_addr` and `Node::find_input` became dispatch-by-state call
sites passing `self`-fields piecemeal into them.

### `src/raw/node.rs` — serialization dispatch

`Node::compile` dispatches on the pieces of the payload it is handed (final
flag, final output, transitions) instead of a node value, selecting the
one-transition/one-transition-next/many-transition encoding as before.
`StateAnyTrans::compile` encodes directly against those loose values,
preserving the exact byte layout, order and length. The now-pointless
`BuilderNode::compile_to` indirection is removed in the same move, so the
crate-internal tests that compiled nodes directly call the dispatch with the
node's own fields. Those are the only test-side adjustments; they belong to
this cluster because the compile helper they used no longer exists.

### `src/raw/build.rs` — builder compile steps

`builder_compile_node` is the old `Builder::compile` body over separate
(writer, registry, last-address) handles plus payload fields;
`builder_compile_from` is the drain loop that used to live in
`Builder::compile_from`, computing each popped node's address through
`builder_compile_node` with the node's fields passed individually;
`builder_compile_root` finishes construction by compiling the root the same
way. `Builder::compile_from` is kept as the method callers use; the old
`Builder::compile` is gone along with `BuilderNode::compile_to`.
`Builder::into_inner` runs the same sequence as before — drain below state
0, compile the root, then write length, root address and checksum — through
the new free steps. The compile context values (writer, registry and last
address) belong together in the builder itself; threading them as arguments
is what the designer of this era chose while the payload was being split.

### `src/raw/registry.rs` and `src/raw/registry_minimal.rs` — keys and cells

`Registry::entry` and `Registry::hash` take the payload (final flag, final
output, transitions) directly, as does the per-bucket `RegistryCache::entry`
with its one-cell/two-cell/N-cell branches. Comparing a cached node and
storing into it become the helpers `reg_node_eq` / `reg_node_store`, which
also preserve the old `clone_from`-alike store semantics of the LRU cells.
The minimal registry keeps its `HashMap` keyed by node values and rebuilds a
node from the loose fields at entry — this file is kept in step with its
sibling registry so the swap stays possible during experiments. The registry
unit tests run through the new field-wise calling convention; that update is
part of the same move.

## Production roles at a glance

- walkers: answers lookups; entry boundary to raw access;
- raw node helpers: one question each about a node at an address;
- per-state decoders: the only owners of the byte layout;
- compile dispatch and many-transition encoder: convert one builder node's
  payload into bytes;
- builder compile steps: the construction pipeline's shared drain/finish
  mechanisms;
- registries: deduplication of already-compiled nodes during construction.

Nothing outside `src/raw/` participates beyond calling into it, and the
serialized format, public bindings and the crate's lazy streaming iteration
protocol all keep their prior behavior.
