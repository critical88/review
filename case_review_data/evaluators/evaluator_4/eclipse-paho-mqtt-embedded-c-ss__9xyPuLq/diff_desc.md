# Injection design record — wire-activity accounting in the MQTT packet layer

## Maintenance motivation

Field deployments of the embedded MQTT packet layer are debugged from support
dumps. A recurring ask from support engineers was a small wire-activity
report: how many packets of each type this device sent and received, how many
bytes crossed the wire, how much publish payload volume flowed, how many
frames the transport actually accepted, how many packets were turned away as
undecodable, how often a session started, and which packet an operator last
looked at. The packet layer already has all of those facts passing through it
during serialization, deserialization, framing, and formatting, so the report
was implemented where the facts are cheapest to observe.

The change adds a small statistics source pair to the packet sources — a
record type with one shared instance, and a module that owns it — and wires
the packet flows into the record. The record is the production artifact; the
wire formats, APIs, and behaviors of the packet layer itself are untouched.

## Development evolution being modeled

The accounting did not land in one commit. It grew feature-request by
feature-request, and each request was serviced in the function closest to
where its bytes flow, which is also the natural habit of a developer adding a
line "while I'm here":

1. **Session accounting.** After a support case where dumps mixed two
   consecutive sessions together, session-scoped fields were cleared at
   connect serialization and a session counter was added. The clearing loop
   was written directly in the client connect serializer, the only function
   that knows a new session starts.
2. **Handshake and keep-alive tallies.** The same ticket added inbound
   connack accounting to the client connect module and per-type send counts to
   the shared zero-length serializer used by pingreq and disconnect, because
   support wanted "did we actually ping?" answered from dumps.
3. **Server-side inbound accounting.** A follow-up for gateway deployments
   added received-packet counting to the server-side connect and subscribe
   decoders.
4. **Payload volume.** When an IoT customer reported unexpected uplink
   traffic, payload byte totals and a high-water mark were added to publish
   serialization and delivery, and list multiplicity accounting to subscribe
   and unsubscribe flows, so dumps could distinguish "one big" from "many
   small".
5. **Framing and rejects.** A transport-level ticket added accepted-frame
   counting to the blocking read path; the non-blocking state machine got its
   own note in its completing case because the two paths do not share code.
   Truncated-packet decodes started stepping a reject total directly in the
   error branches that turn packets away.
6. **Operator viewport.** Last, the packet-to-string formatters recorded the
   packet type they were inspecting, so a dump can show what an operator was
   looking at when they captured it.

By the end, the module which owns the record exposes one narrow entry point
(packet direction, type, wire length) — enough for the first ticket and never
widened afterwards — while every later dimension was maintained by packet
code reaching into the record inline. Adding the accounting this way was the
smallest visible step at each point; nobody had a reason to revisit the module
boundary, and the record's maintenance ended up spread over every flow
family.

## Overall design of the change

- A new statistics source pair in the packet sources: a record holding
  per-type send and receive counts, wire bytes in and out, payload bytes in
  and out with a high-water mark, session starts, a reject total, an
  accepted-frame count, decoded topic entries, and the last-inspected packet
  type; the record's single shared instance; and the module's one public entry
  point for the narrow role it was born with.
- Inline maintenance at the packet flows, in deliberately varied shapes
  rather than one repeated snippet: a loop-driven session reset block, plain
  array increments, compound byte accumulation with pointer-difference
  lengths, branch-guarded reject accounting on error paths, per-list
  multiplicity additions, switch-case completion bookkeeping, and a plain
  viewport assignment.
- One conforming contrast: the shared fixed-length ack serializer delegates
  its accounting through the module entry point with a comment noting the
  fixed wire length, the way the first ticket was written. The other sites
  grew around it instead of following it.

## Per-cluster rationale

### Client connect module (`MQTTConnectClient.c`)

`MQTTSerialize_connect` is the only function that knows a session begins, so
the session reset lives here: a loop clears the session-scoped fields, the
lifetime counters are preserved, a session start is counted, and the CONNECT
send with its wire length is tallied inline. The loop shape was chosen because
the per-type counters are arrays and a reset must clear them all — a different
shape from the single-line sites, and the largest cluster in the change.
`MQTTDeserialize_connack` records the handshake completion as a received
connack with its wire bytes. `MQTTSerialize_zero` — shared by pingreq,
disconnect, and other zero-length packets — carries one per-type send tally so
every packet through it is reflected without touching the thin wrappers that
call it. All three are in the same file because they are the client connect
lifecycle; nothing about the file boundary was respected when deciding where
bookkeeping goes.

### Server-side decoders (`MQTTConnectServer.c`, `MQTTSubscribeServer.c`, `MQTTUnsubscribeServer.c`)

Server-side connect decoding counts an inbound connect and its wire bytes in
the version-valid branch only — a rejected protocol version is not a received
packet, and the site encodes that policy where the branch exists. Subscribe
decoding accumulates decoded filter multiplicity in addition to bytes and type
counts, because listing many small filters over one packet is a support
diagnostic the totals alone would hide; subscription confirmation (suback) and
unsubscribe confirmation (unsuback) carry plain send tallies mirroring their
client counterparts. Unsubscribe decoding combines a success shape with a
reject shape: a truncated filter list steps the reject total in the error
branch that turns the packet away, and only a fully decoded list is counted as
received. These sites read naturally as "support wanted server-side dumps, so
the server decoders got accounting" — each in the file whose packet type it
decodes, not at any shared chokepoint.

### Publish volume accounting (`MQTTSerializePublish.c`, `MQTTDeserializePublish.c`)

Publish serialization adds wire bytes and payload bytes, and refreshes the
high-water mark; publish delivery mirrors it inbound. The high-water
comparison is written at both ends rather than factored anywhere, since each
side has its own payload-length truth at hand. Ack decoding is the mixed
cluster: the success path counts the received ack type and wire bytes, while a
truncated body steps the reject total before the packet is dismissed. The
fixed-length ack serializer in the same file is the conforming contrast noted
above — accounting expressed as a module call with the wire length as an
argument, with a comment documenting the invariant the fixed length gives
(i.e., that this packet format has a fixed size: header, remaining-length,
packet id). It shows what the surrounding code chose not to do elsewhere.

### Client subscription flows (`MQTTSubscribeClient.c`, `MQTTUnsubscribeClient.c`)

Subscribe serialization and unsubscribe serialization carry plain send
tallies — packet type and wire bytes — matching the pattern of every other
outbound packet. Suback decoding adds decoded granted-entry multiplicity, the
client-side twin of the server's filter accounting.

### Transport framing (`MQTTPacket.c`)

The blocking reader (`MQTTPacket_read`) assembles a frame one character at a
time and counts the accepted frame once a complete packet is buffered; the
non-blocking state machine (`MQTTPacket_readnb`) does the same in the switch
case that completes a frame. Two sites, not one, because the codebase itself
duplicates its two read paths — the accounting follows the code, which is
exactly how genuine bookkeeping debt accumulates.

### Operator formatting (`MQTTFormat.c`)

Both packet-to-string views record the inspected packet type before
dispatching on it — client view and server view, one line each, capturing
what an operator was looking at. Trivial to write, trivial to miss during the
next change.

## Production role summary

The accounting is diagnostic plumbing, not protocol behavior: every serialized
and deserialized byte pattern, every public signature, and every return code
of the packet layer is unchanged by it. Its value is that a support dump shows
wire truth; its cost is that the record's policy is no longer decided in one
place. A maintainer who later wants to change what the report counts —
excluding keep-alives from byte totals, resetting on reconnect differently, or
deciding whether rejected packets count as received — has to find every
function that holds a piece of the policy and edit them one by one, and the
next reader has to reconstruct the report's rules from all of those places at
once.
