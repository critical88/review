# Injection design record — orphaned RFC 3489 compatibility implementation

## Maintenance motivation (why this change exists in a real codebase)

coturn documents the staged removal of its RFC 3489 ("classic STUN",
magic-cookie-less) Binding compatibility mode in
`docs/rfc3489-deprecation.md`. The plan is explicit: RFC 5389 deprecated the
mechanism in 2008, RFC 8489 dropped it, the option carries a startup warning,
and the whole path is "scheduled for removal in the next major release, with
no replacement". The removal is a real maintenance step the project committed
itself to, not an artificial exercise.

Real removals of a cross-cutting option are rarely landed in one commit. The
option thread is woven through the whole server start path — the CLI
surface, the global parameter block, the relay-thread server setup, the
per-session state — before it ever reaches the code that answers a request.
When the removal is split (or simply cut short), the natural seam is between
"stop accepting/advertising/selecting the mode" and "delete the
implementation that the mode selected". This diff models exactly that
state: a developer who landed step one of the documented plan — every place
that *reaches* the mechanism — and left the implementation side for a
follow-up that never came.

## Evolution being modeled (how a supported feature actually dies)

The change is a behavior-preserving *removal*, which is the honest shape for
this mechanism in this repository because the flag is off by default and
documented as unsafe to enable: with the entry points gone from a default
build, no supported configuration distinguishes the tree before and after
this diff at runtime. The modeled developer task — "expose, gate, then
dispatch on the compatibility flag" — is being undone in the same direction
it was originally built. What remains after the modeled step is the retired
mechanism's implementation kept in the source with no way to reach it.

The removal deliberately stops before the shared wire library. coturn's
`src/client` message builders are public API: they still accept and emit the
legacy format on request, and the repository's own unit tests exercise those
legacy constructors directly. Deleting those would change a supported API
rather than finish an internal cleanup, so a staged removal taking the
process seriously keeps them and leaves the boundary between transport
encoding (shared, retained) and server policy (retired, removed).

## Overall design

The injection touches the six files the option thread actually passes
through. In every case the deleted code is the *entry side* of the
mechanism: option parsing, state plumbing, dispatch, classification. The
*implementation side* — the legacy request handler and the marker locals that
fed the removed branches — is left in place, after which no path under any
configuration reaches it.

## Per-location rationale

### `mainrelay.c` — CLI and configuration surface (4 clusters)

1. **Initializer of the global parameter block** (removed line
   `false, /* rfc3489_compatibility */`). The global `turn_params_t` is the
   process-wide snapshot every worker thread reads. Removing the field's
   initializer entry (together with the field itself, below) is the
   canonical first edit of an option removal: nothing can set or read the
   value anymore.
2. **Usage text** (removed help entry). The help block is hand-maintained
   per option; leaving a scheduled-for-removal option documented would
   directly contradict the removal, and the real plan's step one removes
   the advertising.
3. **Option machinery** (enum value, `long_options[]` entry, `set_option`
   case). This cluster is the entire accepting surface of the flag: without
   it, `--rfc3489-compatibility` stops parsing as a known option. These
   three edits are one semantic unit — an option is name, table entry, and
   handler or it is nothing — so they are removed together. The role is
   request-path-adjacent configuration, and the selected shape (strip the
   whole accept path rather than keep a rejected flag) matches how sibling
   deprecated options are advertised as removable.
4. **Startup deprecation warning block** (`main()`). The warning existed to
   tell operators the flag was scheduled for removal. Once the flag cannot
   be set, warning about it is unreachable advice; a removal that deleted
   the flag but kept its warning would be visibly half-done in the most
   user-visible function of the file. It contains the only "set" of the
   removed parameter field, so it also eliminates the last writer.

### `mainrelay.h` — parameter block definition (1 cluster)

The `turn_params_t` field declaration. Retained, it would be an unwritten,
unread member forever; removing it is part of the same semantic edit as the
initializer line, and the header is where the next reader discovers what the
server can even be configured to do.

### `netengine.c` — relay-thread setup (1 cluster)

The `init_turn_server()` call passes the global flag's address into each
relay thread's server struct. Options plumbed by pointer exist so that live
reconfiguration can reach worker state; for a flag whose only consumer is
about to be a dead branch, the pointer is pure plumbing. Removing the
argument (with the parameter below) is the "stop threading the setting into
the runtime" step, and choosing the call site rather than only the
definition is what keeps the file's transports honest: the call is the entry
into the relay component.

### `ns_turn_server.c` — server state, dispatch, and initialization
(3 clusters)

1. **Per-session dispatch branch** (`read_client_connection`, the `else if`
   recognizing cookie-less requests and calling the legacy handler). This is
   the mechanism's only runtime entry: the branch that decides a datagram is
   a legacy request and hands it to the handler. Deleting the branch removes
   the reachable path; the handler and the cookie local that fed the branch
   remain — left behind by the modeled incomplete step — and the local is
   now declared and never read.
2. **`init_turn_server()` signature and body** (parameter removed;
   assignment to the server field removed, matching the header change
   below). The server struct kept a per-copy flag that now nothing sets,
   and the function kept a parameter every caller had to thread through.
   Removing them is the server-side half of the plumbing removal started in
   `netengine.c`; a coherent removal updates definition, header prototype,
   struct, and call sites in the same step, which is why these edits spread
   across the boundary between the two files.
3. **The legacy handler itself** (`handle_old_stun_command`) is *not*
   touched. That is the point of the modeled incomplete step: it is a
   complete request-processing implementation — parsing, legacy-form
   binding/error response construction, legacy server-attribute attachment
   — that after this change has no caller anywhere in the runtime. Its
   supporting marker locals in the dispatch loop retain only their
   declarations. Held together with the entry-side removal, the source now
   describes a mode the binary can no longer enter but still carries the
   code for.

### `ns_turn_server.h` — server contract (1 cluster)

The per-server flag field (with its deprecation comment) and the
corresponding prototype parameter. Headers document the operator- and
component-visible contract; leaving a delete-before-use field in the struct
would keep advertising a capability the server no longer has. This is the
declarative half of the same edit as `netengine.c`/`init_turn_server`, which
cannot be applied one without the other in a language whose type definitions
must stay consistent with every use.

### `dtls_listener.c` — listener fast path (1 cluster)

The classification branch in `classify_udp_packet()` that recognized
cookie-less datagrams as a distinct class (gated on the removed global
flag). The UDP listener must classify every arriving datagram before any
session exists, so the compatibility mode also had to live here; this is the
mechanism's second and structurally different entry point — the same removal
one component away, not a repetition of the same edit. The branch is
removed from the classifier chain while its supporting cookie local and the
classification category it produced stay behind; the local is now declared
and never read. The pre-filter that tolerates one legacy-style attribute on
modern requests is *not* touched: it serves live, post-3489 message
parsing on the modern path.

## What this change does not touch (and why that is the honest boundary)

The shared wire library (`src/client`) keeps its exported legacy-format
constructors, its magic-cookie-less message helpers, and the legacy
attribute macros: they are public API consumed by the repository's own test
suite and by external software, and none of them becomes unreachable — the
tests still call them. The modern binding-request handling keeps its
tolerated-attribute parsing. Those sites are the difference between
"retire a server policy" and "break a wire-format library", and a staged
removal must respect that boundary.

## Coverage of the modeled step

Six production files across four responsibilities — CLI/config surface,
global state, runtime plumbing, request dispatch, and listener
classification — all genuinely participate in the same option thread. The
removals are all of the mechanism's reachable entry sides; the unchanged
sites are the implementation and its state, which after this change is
inconsistent with the surrounding source: present, maintained, and
unreachable.
