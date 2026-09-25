# Injection design record: peer reply-endpoint handling in the serf core

## Maintenance motivation

Serf's query machinery must always know, for a message it may have to answer,
*where the answer goes*: a UDP address, a port, and the name of the peer node.
The repository already owns several bundled forms of that value -- the wire
structs carry the originator's address/port/name as fields, the relay header
carries a `net.UDPAddr` plus a destination name, member records carry a
member's address/port/name, and `memberlist.Address` exists precisely to name
a peer by address and name together.

The work modeled here is the state a maintainer reaches when that value keeps
being taken apart instead. Two concrete pressures push the code in this
direction and are what this change is staged against:

1. **Every extracted helper re-declares the pieces it needs.** Serf grew its
   query lifecycle incrementally -- queries, then delivery acknowledgments,
   then relayed-response redundancy, then lock-hygienic extraction of the
   reconnect attempt. The cheapest way to wire each new helper is to pass
   precisely the values it consumes, copied from whatever struct sits at the
   call site. The address/port/name group therefore reappears as loose
   parameters everywhere, each with names borrowed from local context.
2. **The pieces drift as they travel.** The same logical address arrives as
   `[]byte` on message-derived paths and as `net.IP` on member-derived paths;
   the same logical port is `uint16` in the wire and member structs but `int`
   in the UDP-layer calls every destination eventually feeds. The
   conversions happen at hop boundaries, one site at a time, so no single
   place shows the full cost.

A developer extending reply handling (for example, giving relayed responses
their own acknowledgment, or attaching sender identity to reply destinations)
has to thread the trio through signature after signature before any new
behavior can be added, and has to do the same thread-twisting again later when
the next hop is added. That repeated plumbing is the maintenance friction this
state captures.

## Development evolution being modeled

The change is shaped as the output of ordinary extract-and-parameterize work
on an already layered pipeline:

- A **direct-UDP delivery helper** is extracted because both the new ack
  writer and the existing response writer need the same one-shot send; each
  writer passes its origin endpoint pieces.
- An **ack writer** is extracted from the query-ingest branch so that
  acknowledgments stop being inlined in message dispatch; it needs the
  query's origin endpoint and its relay factor.
- A **response writer** is extracted from the event-side response path, which
  previously did its own direct send and relay; the extraction takes over the
  endpoint and relaying for both writers.
- **Query construction** gains a message-assembly helper in front of the
  broadcast step, taking the origin endpoint (and the rest of the query
  fields) as separate values, mirroring how every other struct here gets
  built next to its call site.
- **Client event construction** becomes a helper so the ingest branch stops
  building the event inline; the client answers through that event later, so
  the endpoint pieces travel into it.
- **Relay forwarding** in the memberlist delegate is given its own helper so
  the message-dispatch switch stops reaching into networking directly; the
  decoded relay header's destination is handed over piecewise.
- **The reconnect attempt** is extracted from the periodic sweep to narrow
  the member-lock scope; the extracted function naturally receives the member
  fields it consumes, including the address/port/name.

Each extraction copies the values it needs from the owner in scope -- a wire
message here, a member record there -- which is why the group never acquires
one name, one type, or one owner along the way.

## Overall design

One boundary shape recurs across the pipeline: **wherever the reply
destination moves between owners, it moves as separate address, port and
node-name parameters**, with identifiers chosen from the local vocabulary of
the sites involved (`origin*` near query ingest, `reply*` near writers,
`dest*` at the relay header, `member*` at member state) and with types copied
from the struct at hand (`[]byte`/`uint16` from the message lineage,
`net.IP`/`int` where member and UDP-layer shapes dominate). Concretely:

- Delivery helpers take the endpoint pieces; the endpoint directions
  (`net.UDPAddr` construction, `memberlist.Address` construction) stay at the
  far end since that's where the send actually happens.
- The message/relay/event/member structs keep their bundled fields and remain
  the sources and sinks of the scattered values.
- Address/port conversions are kept at hops where the source struct and the
  destination call disagree on width/representation, matching what a
  copy-at-the-callsite evolution leaves behind.
- On the shared response-send path, the Lamport time and message id travel
  *beside* the response object that already carries those same fields --
  exactly what joining the (previously separate) ack and response writers
  into one shared helper would produce.

No wire struct, no exported type, no exported function signature, and no
log format string is altered by the change; the group's movements are all
inside the `serf` package's internal call chain.

## Design record per cluster

### Cluster A -- Query issuance (`serf/serf.go`)

`Query()` builds the outgoing query through a new assembly helper. The
originator endpoint (local member address, port, and node name, read from
local member state) is passed to the helper as separate values alongside
filters, flags, timeout and payload, and the helper stamps them into the
message fields. *Why this site*: query issuance is the moment the
originator's own reply destination first enters the pipeline, so a
message-assembly seam here is the natural place for the pieces to be at their
most exposed. *Role*: creates the origin endpoint data every later hop in
this task consumes.

### Cluster B -- Query ingest and acknowledgment (`serf/serf.go`)

The query-dispatch branch now routes through two new helpers: an ack writer
that formats and sends (and relays) a delivery acknowledgment without waiting
for a client response, and, when the ack has been sent or when the query is
otherwise processed, an event construction covered in Cluster D. The ack
writer takes the query's carried endpoint pieces as its own parameters, then
hands them onward to the shared delivery and relay helpers (Cluster C/E) --
twice re-declared within one node of dispatch. *Why this site*: the ack path
is the purest read-then-rewrite of endpoint data in the codebase: values
come off the wire message, get passed as loose parameters, get converted
from `uint16` to `int` at the send seam. *Role*: pull-side ingest of
originator data with immediate write-back through the ack.

### Cluster C -- Shared direct delivery (`serf/serf.go`)

The direct-UDP delivery helper takes encoded bytes plus address/port/name,
builds the UDP address and memberlist address, and performs the one-shot
send. It serves both the ack writer (Cluster B) and the response writer
(Cluster D/F). *Why this shape*: both writers need an identical send, so the
extraction is the natural shared seam; carrying the pieces as parameters is
what "pass just what you need" yields. *Role*: the single place a direct
(non-relayed) reply actually leaves this node for a peer.

### Cluster D -- Client event hand-off and response writer (`serf/serf.go`, `serf/event.go`)

Ingest now constructs the client-facing query event in a dedicated helper,
stamping the message-carried endpoint pieces onto the private event fields
(the event keeps exposing only the source node and the deadline). The event
side (`respondWithMessageAndResponse`) keeps its size-limit, lock and
deadline discipline, but its send-and-relay tail now calls the shared
response writer, passing the event's stored endpoint pieces (plus the
redundant Lamport time/id, see below). *Why this site*: the event is the
point where endpoint data crosses from message handling into a client-owned
value; the extraction of the shared writer is where the response path and
ack path converge, accumulate their overlap, and re-declare the pieces again.
*Role*: hands endpoint data to whoever the client later delegates the reply
to, and gives the response path its target.

### Cluster E -- Relayed-response duplication (`serf/query.go`)

The relay helper wraps an already-encoded response/ack for the originator at
the given address/port/name and then forwards copies through other members;
the per-hop send is its own helper taking a member's address/port/name from
the member record in the loop. *Why this site*: duplication is the path where
the group is simultaneously (a) consumed to build the originator refill
header, (b) re-sourced from member state for each relay hop, and (c)
converted between `uint16` and `int` on both sides; a relay-aware extract
passing just the members needed is precisely what leaves this trail.
*Role*: replies-as-redundancy; the only stretch where the group is consumed
twice with two different sources inside one function body.

### Cluster F -- Relay ingest and forwarding (`serf/delegate.go`)

The memberlist message delegate unwraps relayed payloads: the relay header's
destination address, port and destination name are pulled out into three
locals and handed to a forwarding helper that rebuilds the UDP/memberlist
address at the far end. *Why this site*: the relay header is an
already-bundled destination that this path takes apart in the middle of a
dispatch switch; the helper extraction that kept networking out of dispatch
is the plausible author of the piece-wise hand-off. *Role*: member-to-member
relaying of responses toward the originator.

### Cluster G -- Failed-member rejoin (`serf/serf.go`)

The periodic reconnect sweep extracts the actual rejoin attempt into a
dedicated helper so the member lock can be released before dialing. The
sweep reads the failed member's name, address and port under the read lock,
releases it, then passes the three values (here `net.IP`/`uint16`, matching
the memberlist node record they came from) into the helper, which rebuilds
the join target string. *Why this site*: rejoin is the destination group's
non-query manifestation -- same three values, different lifecycle purpose
(failure recovery rather than answering), best type evidence of the `net.IP`
side of the address drift. *Role*: shows the group is not a query-only
pattern but the general "where to reach this peer" habit.

## Representational drift, deliberately kept

The scattered group deliberately does **not** normalize its types: the
wire-lineage sites carry `[]byte`/`uint16`, the member-lineage sites carry
`net.IP`, one writer port is `int` while a sibling's is `uint16`, and
`uint16`→`int` conversions sit at the sends. That is the drift a
copy-from-nearest-struct evolution actually produces, and it is what makes
the group's wander through this code legible: each site's parameter types
remind you of the struct the values were lifted from. The cluster naming
(`origin*`/`reply*`/`dest*`/`member*`) likewise reflects each site's
vocabulary rather than one chosen vocabulary.
