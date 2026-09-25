# Injection design record — device control-plane decomposition wave

## Maintenance motivation

The `device` package of wireguard-go is the WireGuard state machine. It carries three
long-lived code paths that had each grown into a single monolithic routine:

- the UAPI **set** operation (`device/uapi.go`), which parses a configuration stream
  line by line and applies it to the device and its peers;
- the **inbound handshake handling** loop (`device/receive.go`), which takes queued
  handshake datagrams off a channel and validates, rate-limits, and consumes them;
- the **Noise handshake transcript** consumption (`device/noise-protocol.go`), which
  turns an initiation or response message into an authenticated session.

Each of these routines mixes queue or scanner mechanics with protocol decisions in one
body, which makes per-concern changes hard to review and per-branch behavior hard to
exercise in isolation. A maintainer wave to decompose them into smaller stages is the
motivation modeled here. The repository's own history prices this exact work on the
UAPI side: upstream commit `6252de0` ("device: split IpcSetOperation into parts")
factorized precisely that traversal in 2017 by giving the per-operation peer state a
small struct. The wave modeled here reaches the same maintenance milestone along a
different route: it splits the same routines, but resolves the state that crosses the
new method boundaries by threading it as explicit parameters rather than by giving it
a shared home.

## Modeled development evolution

The diff reads as a short series of individually reviewable changes, in the order a
maintainer would naturally make them:

1. `device/uapi.go` — the set-operation traversal is split into per-line stages and a
   post-configuration stage, with the traversal's working state passed to each stage.
2. `device/receive.go` — the handshake handling loop is reduced to queue mechanics and
   dispatch; per-message-type processing moves into dedicated stages, with MAC and
   under-load validation extracted into a shared gate in front of consumption.
3. `device/send.go` — the cookie-reply constructor stops taking the queue element and
   takes the two inputs it actually needs.
4. `device/noise-protocol.go` — initiation consumption is staged into named transcript,
   decryption, and timestamp-adjudication steps, and the response path's inline stage
   becomes a named method.

Each change is defensible on its own, and the wave stops at the natural review boundary
for a decomposition: stage structure moves, observable protocol outcomes do not.

## Overall design

The decomposition keeps the caller skeletons in place and moves body logic into new
methods:

- **Configuration plane.** `IpcSetOperation` keeps the scanner loop and line parsing,
  but a `public_key` line now closes out the previous peer and hands off to a
  public-key stage, other peer lines are applied by a peer-line stage, and completion
  of the operation (or of a peer section) is applied by a post-configuration stage.
  The traversal's peer being configured, its dummy/placeholder provenance, its
  created-during-this-operation provenance, and the keepalive-was-turned-on signal
  cross these boundaries as explicit pointer parameters.
- **Inbound control plane.** `RoutineHandshake` keeps the queue loop — including
  returning the message buffer to the device pool exactly once per element — and
  dispatches each element by message type to a dispatcher method. The dispatcher
  routes cookie replies to their consumer, and for initiations and responses first
  runs a shared authentication gate (MAC1 check; under load, MAC2 check plus per
  source address rate limiting, with an owed cookie reply short-circuiting
  consumption) before handing the datagram to the per-type consumer.
- **Cookie reply.** `SendHandshakeCookie` builds the reply from the denied packet
  bytes and the remote endpoint only.
- **Transcript plane.** `ConsumeMessageInitiation` walks its named stages — mix the
  transcript, decrypt the static, adjudicate the timestamp — and keeps the existing
  peer-handshake lock domain, index-table update, and zeroization of the working
  arrays at commit. `ConsumeMessageResponse` now delegates its inline transcript
  stage closure to a named method.

## Per-location rationale

### `device/uapi.go` — configuration traversal split

**What changed.** The traversal now declares the peer being configured, its
dummy/placeholder provenance, its created provenance, and the keepalive flag as local
variables, and passes them by pointer into each new stage: the post-configuration
stage (deferred peer start actions), the public-key stage (peer selection/creation for
a new section), and the peer-line stage (per-key application). The earlier
struct-based grouping that the pinned revision still carries is gone; each stage
names the values individually in its signature.

**Why this site.** This is the package's largest routine and the one whose per-line
switch had already attracted upstream decomposition effort once; it is the highest
value target for "make each protocol line's handling reviewable on its own".

**Why this shape.** The traversal state mutates as lines are consumed (a
`public_key` line replaces the peer being configured, an `update_only` or `remove`
line reverts it to a placeholder) and the traversal must observe those transitions
afterward, so the stages take the state by pointer rather than returning it, and the
traversal keeps its locals rather than introducing a new type. This is the classic
shortest-path split: extraction without redesign of the state's ownership. The
keepalive signal is threaded to the two stages that need it and not the third, which
is a consequence of where it is produced and consulted.

**Production role.** This plane carries the entire configuration surface exposed to
users via `wg-quick`/`wg`-style tooling; its semantics (errno discipline, placeholder
peers for the device's own public key, deferred start actions, roaming suppression
when the platform needs it) are contract, so the decomposition is careful to preserve
each branch's outcome and logging verbatim.

### `device/receive.go` — inbound handshake handling split

**What changed.** The handling loop now only takes elements off the channel,
dispatches by type, and returns buffers to the pool. A dispatcher method routes
cookie replies directly, and gates initiation/response consumption behind a shared
authentication method; the per-type consumers each hold the unmarshal, consumption,
and reply logic that used to be branches of the loop's switch.

**Why this site.** The routine mixed four unrelated concerns — queue mechanics,
message-type dispatch, under-load DoS policy, and per-type message consumption — and
each branch contained a full protocol path. Staging it makes the DoS ladder and each
message path independently reviewable.

**Why this shape.** The packet bytes and the remote endpoint are the datum pair every
downstream decision needs (logging attribution, MAC validation inputs the packet;
rate limiting and replies need the source address), and both are read-only
downstream, so they travel by value into every stage rather than being re-read from
the queue element. The dispatch parameter stays on the dispatcher alone because only
dispatch consults it. The under-load cookie reply is issued from the authentication
gate, exactly where the MAC2 verdict happens, keeping the deny-early posture. The
`goto`-skip flow of the old loop is replaced by early returns in the extracted
consumers, which is behaviorally equivalent for these paths and reads better at
stage granularity.

**Production role.** This is the entry point of the control plane for everything
peers send that is not data traffic. The order of the validation ladder (MAC1 before
anything; MAC2 only under load; rate-limit allowance before consumption; cookie
replies consumed rather than answered) is security-relevant contract.

### `device/send.go` — cookie-reply construction follows the split

**What changed.** `SendHandshakeCookie` previously rooted its inputs in the handshake
queue element; it now takes the denied packet bytes and the remote endpoint directly,
and its local reply buffer variable is renamed so the packet parameter is not
shadowed.

**Why this site.** With the receive path no longer passing queue elements around
internally, the reply constructor's signature would otherwise dangle on queue
internals it does not need. Taking only the bytes it roots and the source it must
answer is the minimal, self-contained signature for the operation — and by the time
the reply is owed, the queue element it came from is being recycled to the buffer
pool, so holding the whole element would only suggest a lifetime it does not have.

**Production role.** This is the wireguard DoS defense's answer: a signed cookie
reply that lets a legitimate peer retry under load without letting an attacker
consume memory.

### `device/noise-protocol.go` — transcript staging

**What changed.** Initiation consumption is reframed as three named stages over the
message and the transcript working state: transcript mixing, static decryption, and
timestamp adjudication (replay and flood verdicts derived from the peer's handshake
history). The response path's inline transcript stage closure becomes a named method
over the response message, the peer handshake, and the same working state.

**Why this site.** This body is the most security-dense in the package: it derives
the peer's identity, decides replay, and hands off to session derivation. Staging it
makes each cryptographically meaningful step nameable and individually reviewable,
which is what an auditor of this protocol actually wants to read.

**Why this shape.** The transcript is two arrays working in tandem: the mix hash and
the chain key must advance together through every stage for the Noise transcript to
be sound, so the stages receive the working state by pointer into the same arrays
the caller prepared, alongside the read-only message view each stage needs (and, for
the adjudication stage, the peer handshake whose timestamp bookkeeping it updates
and consults).

**Production role.** This is the gate that decides whether an initiation or response
is worth a session; the commit-under-lock, index-table discipline, and zeroization
of hash and chain key material at consumption must not move.

## Deliberate structural variation

The three decompositions deliberately do not resolve their state threading the same
way, because the three planes have genuinely different data semantics:

- the configuration plane threads **mutable per-operation state** by pointer, whose
  members are provenance flags and a selected peer, with one tail member riding only
  where it is produced and consulted;
- the inbound plane threads a **read-only per-datagram pair** by value, which crosses
  a file boundary into the reply constructor, while the dispatch parameter stays put;
- the transcript plane threads **borrowed working memory** by pointer, aliased into
  the caller's arrays, with adjacent views that differ between initiation and
  response staging (initiation stages share one message view; the response stage
  carries the peer handshake view).

Modeling a realistic wave means accepting that different concerns dictate different
styles for moving their context across new boundaries: mutable state asks for
indirection, immutable datum invites copies, and borrowed working state demands
aliasing.

## Flow and behavior continuity

The wave presumes continuity of behavior throughout: queue elements keep their
lifecycle (buffers returned exactly once per element, after handling), the
traversal's deferred actions still fire at the same points with the same conditions,
the DoS ladder keeps its order and its single-reply discipline, the transcript
keeps its zeroization and lock domain, and the wire format is untouched.
