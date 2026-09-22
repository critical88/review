# Re-pointing the TrevorC2 agent's channel is getting scary

I maintain the C implant that ships with TrevorC2 (`agents/c/`). The way this
thing is supposed to work is that the whole channel — the operator's server
address, the port, the cover URL pieces, the cookie name the session rides on,
and the encryption secret — is written down once in the agent's channel
configuration header, and the agent's code just reads it from there. Standing
the implant up against new infrastructure has always meant editing that one
header.

Last week's engagement needed a fresh redirector, so I changed the address and
the port in the header the way I always do. The implant half-moved: some of
its traffic showed up on the new channel, and some of it kept dialing the old
one. It took me an evening to find the first place that was still using an old
value, and I'm honestly not confident I found them all. I've since noticed
the same pattern with the cookie name and I'm worried about the secret too —
at some point bits and pieces of the channel got spelled out again inside the
agent's own code, each pinned by whoever was fixing that part that week, and
nobody ever untangled it. Now a copy can be hiding inside any stage or layer
that talks to the channel — where the beacon does its initial check-in, where
it polls for tasking, where it sends results, in the HTTP code underneath, or
around the payload encryption — and I don't have a complete map.

I want that map gone, in a good way: take a pass over the agent's
channel-facing code and make every part of it follow the channel configuration
again, so that re-pointing the channel or rotating any single channel value is
a one-place edit. Please hunt down everything in the agent that hard-codes its
own idea of the channel, not just the ones I've listed — I'd rather you find
one more than have the beacon stranded on stale infrastructure at 2am again.

Two things please keep sacred: nothing observable about the agent's traffic
may change (same requests, same URLs, same cookie handling, same encryption
interoperability — this must be a pure cleanup as far as the network can
tell), and the existing calling code's contracts should keep working, so the
build stays green and the repo's tests keep passing. I don't want the vendored
AES/base64 guts touched either; they don't know anything about the channel and
I'd like to keep it that way.
