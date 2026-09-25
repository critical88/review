# Injection design record — esp-homekit (deeply inlined method)

## Maintenance motivation being modeled

The TLV (Type-Length-Value) marshaller is the hottest code path in esp-homekit
pairing: SRP and pair-verify messages are large, nested TLV trees, and on an
ESP8266 class target every function call costs stack and flash indirection.
A plausible (and common) pressure on this code base is a latency- and
memory-driven "hot path" sweep: a developer working on pairing latency
flattens the helper chain behind the nested-value entry points so that (a) no
per-record call overhead remains during serialization, (b) the size pass and
the write pass can share one allocation instead of successive ones, and (c)
the stream sink can write directly into the target buffer without a helper
hop. Along the same sweep, the accessory-replication path (deep-copying
characteristic trees for persistence snapshots) gets the same treatment so
that destination-offset arithmetic is expressed once, inline, where the packed
layout is actually computed.

This evolution is realistic precisely because it is *locally* defensible: at
each site the inlined version is correct and no caller-visible behavior
changes. The cost only appears at the module level — the same serialization,
chunking, appending, and copying knowledge now lives twice, once in the
shared sub-methods and once embedded in the public entry points that no
longer call them — so subsequent format changes (chunk size, node layout,
field packing) must be made in two divergent places.

## Normal development evolution being modeled

An incremental performance/ownership sweep across two commits' worth of
work on the same subsystem, not a rewrite:

1. `tlv_add_tlv_value`: the author first fused the "compute flat size, then
   write records" pair of passes into one function with a single flat-buffer
   allocation, absorbing the module's record-formatting pass and the
   record-append step (the flat record must still be linked into the
   destination list).
2. `tlv_get_tlv_value`: the same reasoning applied to the read side — the
   lookup scan, the construction of the destination value list, and the
   chunked-record parse (lookahead across 255-byte continuations) collapsed
   into one body so a remote message turns into nested values without
   intermediate helper frames.
3. `tlv_stream_add_tlv_value`: the streaming sink variant — serialization
   fused with the byte-wise sink discipline (buffer-full check with flush
   callback at every byte boundary) so no per-byte helper call happens.
4. `homekit_value_clone`: on the accessory side, the format-dispatch value
   copy absorbed into the clone entry point, duplicating the per-format
   copy logic (string/data/tlv duplication) rather than calling the copy
   helper.
5. `homekit_characteristic_clone`: the packed-layout migration site — the
   value copy absorbed a *second* time (in a different control shape,
   tuned to the packing loop) and the pointer-alignment arithmetic inlined
   at every offset site so the destination layout is computed where it
   is laid down. The tiny `align_pointer` helper, whose only caller this
   was, is deleted; the general `align_size` helper stays because the
   service and accessory clones still use it.

## Overall design

Two clusters, matching the two production responsibilities:

- **TLV record marshalling** (`src/tlv.c`): the three nested-value entry
  points (add/flatten, get/parse, stream-into-sink) each absorbed the
  chain of sub-methods beneath them — record formatting, list append,
  record parsing, byte-sink put and flush. The sub-methods remain defined
  in the module because other call sites (including code outside the
  module's host-testable surface) still reference most of them.
- **Accessory state replication** (`src/accessories.c`): the two clone
  entry points absorbed the value-copy operation and (at the
  characteristic site) the offset-alignment arithmetic. The upstream
  layout accounting is reproduced exactly, including the historical
  element-size choice at the memcpy/pointer-advance site, so the packed
  destination layout is bit-stable.

Across both clusters the swallowed sub-methods were not deleted, so the
module now carries the same representational knowledge in two
implementations — the drift surface this case is about.

## Per-site rationale and production role

### `tlv_add_tlv_value` — flatten nested values into a flat record

Production role: accessories advertise themselves as nested TLV trees;
this path serializes a nested subtree into one flat TLV record (itself
stored as a value) before wire formatting.

What changed: the module's two-pass formatting (size pass, then write
pass) and the record-append step were absorbed into the entry point. The
size pass and write pass now iterate a `while` list walk with `item =
item->next` tail advance; temporaries renamed away from the formatting
helper's vocabulary (`flat_size`/`flat_data`/`flat_pos`,
`item_left`/`piece_size`) to reflect that the buffer is this function's
own concern now. The per-item 255-byte continuation chunking is
re-expressed locally as a bounded `while (item_left)` copy loop. The
append tail (head insert vs tail walk) was reshaped with a `tail` walker.

Why this site and shape: this is the only module entry point that builds
a *stored* flat record, so both the format pass and the append concern
belong to it; fusing the two passes into one allocation is the concrete
"optimization" payoff the modeled developer was chasing.

### `tlv_get_tlv_value` — parse a flat record back into nested values

Production role: the inverse of the above — reading TLV records carried
inside other TLV values (used while handling remote pair-setup/pair-verify
messages).

What changed: the record-type lookup scan, destination list construction,
and the chunked parse absorbed into one body. The lookahead that groups
consecutive same-type 255-byte continuation records was rewritten around a
`lookahead` cursor with a `total_size` accumulator, and the merge copy
around a `size_left` remainder bound — deliberately different shapes from
the module-level parse routine, which scans with `j` and copies with a
`remaining` bound. Node allocation failure is ignored (matching the module
routine's append behavior); data allocation failure releases the partially
built value.

Why this site and shape: the get path is where message-size latency is
felt during pairing, and it is the deepest local chain
(lookup -> construct -> parse -> append), which is exactly what the modeled
sweep was flattening.

### `tlv_stream_add_tlv_value` — stream a nested tree into a flushing sink

Production role: HTTP-chunked-style output — TLV trees written into a
fixed-size buffer that flushes through a callback (network write) when
full; used when a response does not fit one buffer.

What changed: serialization absorbed again (kept in the `for`-loop shape,
varying from the while-form chosen at the add site), plus the byte-sink
discipline: instead of calling the put/flush helpers, every byte write
performs the guarded `buffer[pos++] = b` with an explicit buffer-full
check, and the flush body (invoke `on_flush`, reset `pos`) is spelled out
at each of the sink sites. The empty-subtree corner still emits the bare
`[type, 0]` header pair through the same inlined sink writes before
returning.

Why this site and shape: this is the highest-call-count path in the sweep
(one helper call per byte before), so it got the fullest absorbing; the
`for`-vs-`while` and put/flush-spelling differences from the add site
mirror how little of the reshaping was planned across functions.

### `homekit_value_clone` — replicate a characteristic value

Production role: deep-copying HomeKit values (format-tagged scalars,
strings, raw data, nested TLV trees) for snapshots and for replicating
accessory state to new service instances.

What changed: the format-dispatch copy operation absorbed into the clone
entry point — `memset` of the destination, flag and format preservation,
and per-format copying: scalar passthrough for the small formats,
`strdup` for strings (written with a ternary), `malloc`+`memcpy` for raw
data, and for nested TLV values an inlined duplication cascade that walks
the source list and re-links copied nodes.

Why this site and shape: the clone path is on the persistence/snapshot
side rather than the wire side; its author duplicated the copy logic here
rather than calling the shared copy helper so the clone "owns" the entire
copy story.

### `homekit_characteristic_clone` — replicate a whole characteristic

Production role: the broadest replication site — a characteristic plus
its metadata, descriptors, valid-value sets and callback slots, copied
into one packed allocation for snapshot/persistence purposes.

What changed: the value copy absorbed a second time, here in a `switch`
dispatch (differing deliberately from the value-clone site's shape) with
an if/else string copy; the offset arithmetic absorbs `align_size`'s
rounding formula at every packing offset, expressed as local
`*_bytes`/`callback_bytes` size computations; and the pointer-alignment
rounding formerly provided by the (now deleted) one-caller static helper
`align_pointer` is expressed inline as modulo arithmetic where `p` is
bumped. The historical element accounting at the valid-values-range
memcpy/pointer-advance site (pointer-size based, differing from the
struct-size used at the size computation site) is reproduced exactly, as
an author paying attention only to "no layout change" would.

Why this site and shape: during a packed-layout migration this function is
where every offset matters, so the author pulled the alignment math inline
to "see" the full layout in one screen. Deleting `align_pointer` (rather
than keeping it for one site) matches the ownership instinct of the sweep;
`align_size` survives because other clones still call it.

## Structural variation, deliberately

The five sites intentionally do not share one uniform rewritten form: the
add site uses `while` walks, the stream site `for` walks; the value-clone
site dispatches with if/else chains and ternary copies while the
characteristic site dispatches via `switch` with if/else copies; error
paths vary between `continue`-shaped and `return`-shaped handling per the
concern being absorbed. This models five hand-driven rework moments rather
than one mechanical template application, and it means any single
uniform transformation cannot capture the whole diff's intent.
