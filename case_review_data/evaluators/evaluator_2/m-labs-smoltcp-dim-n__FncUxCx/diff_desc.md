# Injection design record

## Realistic maintenance motivation

smoltcp's interface and socket code favors flow-through methods on the hot
paths, and the tree already documents deliberate inlining decisions in prose
(upstream comments of the shape "NOTE: we always inline this function into
X"). During the IEEE 802.15.4 and 6LoWPAN work, and in parallel during DHCPv4
lease-handling iterations, the practical way to debug a failing datagram is to
see the whole stage in one place: every jump into a tiny single-caller helper
costs a context switch, and several of those helpers existed for exactly one
call site. The natural maintainer move is to pull the helper body up into its
only caller, keep the flow linear, and delete the now-unused indirection.

The modeled evolution is the ordinary second step of that move: once a stage
lives inside its caller, the *next* medium, address family, or client state
that needs the same work gets a fresh copy of the absorbed body in its own
branch, because the shared function is no longer there to call. Each copy then
drifts independently during later fixes. No single step of this history is
hard to justify in review; the cumulative result is several multi-screen
methods in which distinct protocol stages are interleaved and duplicated per
branch.

## Overall design

The change concentrates on the packet-handling surfaces where single-caller
helpers existed and project direction made absorption plausible:

1. the generic IP egress dispatch path in the interface (`src/iface/interface/mod.rs`);
2. the 6LoWPAN ingress path for IEEE 802.15.4 (`src/iface/interface/sixlowpan.rs`);
3. the 6LoWPAN egress path for IEEE 802.15.4 (same file);
4. the DHCPv4 client socket's packet processing (`src/socket/dhcpv4.rs`).

Every absorbed body was reworked while being moved up — local bindings were
renamed to fit the surrounding names, guard clauses replaced some of the
original early-return helpers, and control flow was reshaped to match the
branch it now lives in — so the moved code reads as written-in-place rather
than pasted. Stage prose ("Reassembly stage", "Decompression stage",
"Compression stage", "Fragmentation plan") was written for the merged flow the
way a maintainer documents a long method they are about to leave behind; it
describes the merged flow and does not mark where the old seams were.

## Cluster 1 — link-layer destination resolution in the IP dispatch path

`InterfaceInner::dispatch_ip` (`src/iface/interface/mod.rs`) is the single
funnel through which every transmitted IP packet passes, regardless of medium.
Before the change it delegated link-layer destination resolution to a helper
that mapped the destination IP address onto a hardware address: broadcast and
multicast addresses map onto derived link-layer addresses, and unicast
addresses are looked up in the neighbor cache, transmitting an ARP request or
an IPv6 neighbor solicitation (depending on medium and address family) when
the neighbor is unknown, which consumes the transmit token and defers the
packet.

The absorbed body is re-created inside the IEEE 802.15.4 branch and again
inside the Ethernet branch, because the two mediums branch on different
hardware-address types and the maintainer of each branch wanted the mapping
visible where it is used. The re-creation reshapes control flow to the branch
it sits in (the Ethernet site drives everything through one match on the
device medium; the 802.15.4 site keeps its own guard order), and the IPv4 and
IPv6 family masks are folded into the branch arms with their
`cfg` gates preserved. The production role is unchanged: unknown neighbors
still defer dispatch through the neighbor cache's rate limiting, and the
transmit token is consumed by the resolution request in exactly the cases it
was before.

This site was selected because resolution is the largest cross-medium stage
in the hottest dispatch function, and because the retained helper test
surface (the in-tree interface test modules exercise the helper by name) made
full removal implausible while branch-local re-creation is exactly what a
maintainer pressed for readability would do.

## Cluster 2 — 6LoWPAN ingress: reassembly and decompression

`InterfaceInner::process_sixlowpan` (`src/iface/interface/sixlowpan.rs`) is the
ingress funnel for IEEE 802.15.4 frames. Its former call chain descended
through fragment reception, fragment-reassembly slot handling keyed on the
link-layer source, datagram tag and size, the reassembly timeout, the
datagram decompression into a shared buffer, and a next-header walk that
expanded compressed NHC extension headers and UDP.

The change absorbs this chain into the ingress funnel. The reassembly guard,
key derivation and timeout arithmetic now live in the reception arms; the
decompression work that used to be performed by a callee is now performed
inside the assembler-feed flow, reusing the shared decompression buffer
directly; the next-header walk was folded into the reception loop with its
per-family arms merged into the surrounding control flow. Where the callee
used to compute an intermediate decomposition and return it, the merged form
threads a whole-buffer alias and takes the leading 40 octets as the IPv6
header — the buffer-handling shape the maintainer arrived at when the stages
stopped being separable. All input-validation exits (unknown dispatch,
unsupported datagram size, full assembler) remain silent drops; no panic site
was added or removed.

This site was chosen because it is the deepest former call chain in the tree
(four or five levels through a set of codec and assembler helpers) and
because the receive path is where the 802.15.4 debugging story makes
whole-stage reading most compelling.

## Cluster 3 — 6LoWPAN egress: sizing, compression and the fragmentation plan

`InterfaceInner::dispatch_sixlowpan` (same file) is the egress funnel. Before
the change it asked a sizing helper whether the compressed datagram fit one
IEEE 802.15.4 frame, then delegated compression to another helper and, in the
fragmenting case, derived its FRAG1/FRAGN plan from a datagram-tag helper.

The absorbed sizing is re-created ahead of the fragmentation decision in the
egress funnel, and its result is reused by the decision and the emission;
tag allocation and the FRAG1/FRAGN plan are computed inline in the
fragmenting arm via a plan block that derives the first-fragment size and the
remaining fragments together; the compressed emission runs directly inside the
transmit callbacks on both the fragmented path (through the interface's
fragmentation buffer) and the non-fragmented path (into the transmit token).
The datagram-size and offset rules (RFC 4944 5.3: datagram size 40 octets more
than the IPv6 payload length; offsets in multiples of eight except for the
last fragment) and the feature-gated hop-by-hop and routing header sizing
are preserved.

This site was selected because the sizing/compression/fragmentation stages
interleave on both send paths, which is precisely where stage separation was
previously load-bearing for reviewers, and where the easiest reading of "one
function per transmitted frame" pulls everything together.

## Cluster 4 — DHCPv4 ACK interpretation in the client socket

`Socket::process` in `src/socket/dhcpv4.rs` handles every inbound DHCP
message. Reception of an ACK in the Requesting state used to delegate to a
parser that validated lease parameters (subnet mask, prefix length, server
identifiers) and assembled the resulting configuration together with the
RFC 2131 renew/rebind/expiry timeline; the Renewing state called the same
parser. The parser existed for exactly these two arms.

The change absorbs the parser once per arm. Each arm keeps its own state
transition and its own config-changed signaling; the lease-parameter
validation early-exits with the existing `net_debug` diagnostics; the
configuration assembly (DNS list cleanup, router address, lease timeline
derivation with the T1/T2 defaults and the configured maximum-lease cap) is
re-created with bindings named to match the surrounding arm. The
Requesting-to-Renewing transition, the preservation of the rebinding flag
on Renewing refresh, and the order of the config-changed flag relative to
state updates are unchanged, because those are the externally visible
behaviors exercised by the socket's contract.

This site was selected because lease timing rules (RFC 2131 defaults, gap
derivations, the maximum-lease cap, and the retry-interval clamps used
elsewhere in the socket) make this the riskiest duplicated arithmetic in the
socket layer,
and because a two-arm duplication is the most natural side effect of removing
a two-caller helper.

## What deliberately did not change

- Public and crate-visible API, feature gates and their cfg combinations:
  unchanged; no new item is exported and no existing signature moved.
- The wire-level packet representations, the neighbor cache and the
  fragmentation bookkeeping in the socket set: unchanged; only call
  relationships around them were rearranged.
- The IPv4-fragmentation region of the generic dispatch path and the
  reassembly-timeout accessors were already upstream and are passed through
  untouched.
- No test, bench, example or documentation file was modified, and the
  in-tree unit tests keep compiling against the same internal seams.
