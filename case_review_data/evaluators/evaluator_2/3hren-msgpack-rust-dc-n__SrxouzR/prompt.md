# Maintenance request: the ext header's two halves keep getting handed around separately

While preparing some ext-related follow-up work on msgpack-rust I kept tripping over the same
thing. Everywhere the workspace writes or consumes a MessagePack `ext` value, the two halves of its
header — the payload length and the application type tag — are shuffled around as two separate scalar
values. The pair is threaded through internal helpers under different names in different places
(`len`/`ty` here, `tag` there), sometimes in the opposite order, and every call site has to repeat
both values in lockstep. Nothing in the code says the two belong together, even though they only
ever mean anything as a pair: one ext header. Reading this code after a few months away, I found
myself re-checking the order and meaning at every hop, and at least one helper receives a length
right next to the payload slice that already carries it.

The problem is not confined to one crate. The low-level primitives crate, the dynamic
`Value`/`ValueRef` model crate, and the serde bridge all have ext paths built this way, and the
serde bridge's deserializer now stores the two halves as separately passed fields as well.

I'd like a cleanup pass over the ext plumbing with repository scope:

- Please investigate everywhere an ext value's header information is passed, stored or forwarded
  between internal functions — on the writing side *and* on the deserialization side — across the
  ext-related parts of the workspace's crates, and make the ext metadata travel as one cohesive
  value rather than as two independent scalars (or stop being passed at all where the data itself
  already determines it). Internal call sites should stop spelling the pair out individually.
- Behavior must be preserved completely: the bytes written for every ext encoding path must be
  byte-for-byte unchanged, and serialization/deserialization semantics must not change.
- Public API compatibility is required: existing public functions keep their current signatures and
  public items stay available. In particular, the long-standing public ext writing primitive in the
  low-level crate keeps taking the length and the type tag the way it always has; how the internals
  represent the pair is yours to design.
- The workspace's normal build and test workflows must keep passing
  (`cargo build --workspace`, followed by `cargo test --workspace --no-fail-fast`).

Please treat this as one coherent pass over the ext paths, and make sure whatever documentation and
comments currently describe the old parameter-by-parameter handoff are brought in line with what the
code does after your change.
