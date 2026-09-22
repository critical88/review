# Remove the leftover RFC 3489 compatibility implementation from the TURN server runtime

## What was observed

coturn documents a staged removal of the obsolete RFC 3489 ("classic STUN",
magic-cookie-less) Binding compatibility mode — see
`docs/rfc3489-deprecation.md`, which schedules the mechanism for deletion with
no replacement. The pinned `turnserver` sources already contain the *outer*
half of that removal, committed as the deprecation plan's first step:

- the deprecated command-line option no longer exists — no help entry, no
  option-table entry, no option handler, no configuration-struct field, and no
  startup warning mentioning it;
- the per-server flag that threaded the setting into the session layer is
  gone from the server state and its initialization signatures;
- the request-dispatch branch that selected the legacy handling, and the UDP
  listener packet-classification branch that recognized cookie-less requests,
  are both gone.

What was *not* removed is the implementation that only this wiring ever used.
The server-side runtime still contains the request handler that answered
cookie-less Binding requests in legacy form and constructed legacy-form
error responses, along with scaffolding that used to feed the now-deleted
dispatch and classification branches. In addition, code that is still live
still carries residue of the retired mode: parameters and marker arguments
that only the legacy path ever needed, passed fixed placeholder values by the
modern STUN request path, and a listener classification category that no
classified datagram can produce anymore.

Under every supported configuration these remnants are unreachable from the
running server, yet they are still compiled, maintained, and read by everyone
touching the surrounding code.

## What to do

Audit the TURN server runtime — the session request-dispatch layer, the UDP
listener fast path, the shared binding-request handling that the modern STUN
path also uses, and anything else the removed wiring used to reach — and
finish the retirement: eliminate every orphaned leftover of the legacy
mechanism so the source carries no implementation, state, or placeholder
residue of it.

The removal must eliminate the obsolete code itself, not neutralize it in
place: a defined-but-unused request handler, an initialized-but-never-read
marker variable, or a branch that exists only behind a fixed placeholder
argument must all be gone, along with any declarations, category values, or
marker plumbing that existed solely to serve them. Where you find code that
still compiles today because live callers pass constant placeholder values,
update those callers consistently rather than leaving the placeholder
scaffolding behind. Nothing outside the retired mechanism may change.

## Compatibility boundary

- Modern STUN (RFC 5389/5489-era) Binding behavior must stay exactly as it
  is: default responses, the backward-compatibility mapped-address form,
  and the RFC 5780 change-request/response-port behavior when enabled. All
  TURN request processing must be untouched.
- The shared STUN message library's public API is out of bounds: its
  exported helpers, wire-format macros, and constructor signatures —
  including the ones that still accept legacy-format parameters — are used
  by the repository's own test programs and by external consumers. Clean up
  the server-side runtime only.
- Where live code deliberately tolerates legacy-style attributes while
  parsing or pre-filtering modern requests, that tolerated behavior is
  supported and must remain.
- The command-line surface of the remaining options is unchanged; the
  documented deprecation plan is not a request to change any other flag.
- The repository's unit test suite must pass unchanged, with no test
  modifications; build and test with the standard CMake workflow documented
  in CLAUDE.md.

## Done means

The server runtime contains no reachable or unreachable implementation of
the retired mode: no orphaned handler, no marker locals, no
placeholder-fed parameters, no classification category that cannot occur,
and no declarations that existed solely for the removed path — with all of
the supported behavior above verified unchanged.
