# TLS material keeps being handed around piece by piece

I maintain the KV service chapters in this repository. The generations that run
the service over TLS have grown increasingly awkward to change, and the reason
is always the same.

The server side of a TLS session needs the certificate, the private key, and
(when mutual TLS is wanted) the CA that vetted the clients. The client side
needs its optional identity pair plus, when we do not want to rely on the
system roots, the CA to verify the server against. These pieces are not really
independent: together they are exactly what a rustls handshake needs, and in
the code it feels like one thing that keeps getting unwrapped into an
argument list. Right now, whenever a secure connection is set up, the pieces
are spelled out parameter by parameter in helper after helper — the ones that
build the session config, the ones that turn a raw stream into a TLS stream,
and the ones that kick off the client connection. In the newest
config-driven generation the same pieces then travel on through the bootstrap:
out of the config structure, into the server startup, down into the
per-connection handler, copying the strings around for every accepted
connection.

This hurts in practice. Adding a field to the key material (say, a key
password, or a second CA) means touching every one of those signatures in
lockstep. The pieces can silently fall out of sync — one call site passes the
identity pair, the next one forgets the CA — and nothing in the types warns
you. Each helper also rebuilds the session configuration from scratch, so
certificate and key parsing errors show up deep inside connection handling
rather than at startup. On the server side it feels wasteful too: the same
certificate material is re-processed for every single accepted connection,
even though it never changes while the listener runs.

## What I would like

Please take a pass over the encrypted KV service generations — the server
accept path, the client connect path, and the config-driven bootstrap in the
latest generation. Investigate how the TLS material travels through them today
and give the concept a proper home: on each side, fold the pieces into a
first-class type that owns loading, validation, and reuse, and pass that one
thing where the loose parameters are currently enumerated again and again.
Address every place you find that repeats the pattern, not just one
representative spot — the point is that later feature work should touch one
type instead of five signatures.

A couple of things should be preserved:

- The externally visible behavior must not change: the same listeners,
  the same CLI binaries and flags, the same wire behavior, and the same
  protocol negotiation the tests assert today.
- The test suite, as the repository's documented commands run it, must
  keep passing exactly as it does now: same green results, no new
  failures anywhere in the workspace.
- The TLS behavior the suite exercises and observes today must survive
  unchanged: the handshakes that currently succeed must keep succeeding,
  and the ones that currently fail must keep failing — for instance,
  connecting under an unfitting server name must still break the
  handshake.
- Keep the trust semantics of each generation as they are: depending on
  the generation, the CA either augments or replaces the default system
  roots, and the client identity remains optional with mutual TLS still
  enforced when the CA is configured.
- Startup-time behavior may improve (fail fast on bad certificate
  material), but connection-time behavior must stay equivalent.

Beyond your changes, nothing else in the workspace should need to change
for the build and tests to be green.
