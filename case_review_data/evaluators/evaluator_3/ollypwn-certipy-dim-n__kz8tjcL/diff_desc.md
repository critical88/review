# Inline flow integration: folding step helpers into the main flows

## Motivation

The tool grew up as a thin command layer over a library of small step helpers:
build a CSR, wrap it in CMC, request the certificate, parse the reply, package
the key pair, build the NTLM message. That composition is convenient to
extend, but through the last several debugging rounds it became inconvenient
to *diagnose*. Tracing one enrollment attempt meant hopping between a flow
method, the certificate library, the protocol helpers and back, with the
interesting interleaving hidden across stack frames.

The maintenance change modeled here is a plausible response to exactly that
friction: for each flow that was actively being debugged, the developer
folded the bodies of the step helpers the flow calls *into the flow function
itself*, so the whole sequence could be read and single-stepped in one
place. The step helpers were deliberately kept — other callers still use
them, deleting them was never on the table — but the flow that absorbed them
now carries its own copy of their implementation, re-bound to flow-local
state.

This is a recognizable style of drift in offensive-security tooling work: integration
debugging pulls implementation knowledge upward into the integrator, the
indirection is restored only partially, and the resulting flows read well in
isolation while quietly re-implementing the library underneath. Later
diff/patch review, blame history, and touch-a-helper-fixes-all reasoning all
degrade, because "the helper" no longer is the only place the behavior lives.

## Overall design

The change touches five flows in five different production files, each chosen
because it is a genuine, distinct entry point into certificate/protocol
machinery with real step helpers available to absorb. The five flows are:

1. `certipy/lib/req.py` — the enrollment client's request method: the
   end-to-end path that builds the CSR, selects and builds CMC wrappers
   (renewal, on-behalf-of, key archival), assembles request attributes,
   submits the request and post-processes the reply.
2. `certipy/commands/forge.py` — the local-CA self-signing path that builds
   and signs a certificate, assembles subject alternative names, and
   packages key + certificate into a PFX container.
3. `certipy/commands/find.py` — the enumeration command's template analysis
   pass, which derives the reported template properties.
4. `certipy/commands/cert.py` — the standalone certificate conversion CLI:
   load certificate/key material, parse PEM or DER, optionally export PFX,
   otherwise emit PEM parts.
5. `certipy/lib/ntlm.py` — the NTLM HTTP authentication retry flow: build
   and send the Type 1 message, compute the Type 3 message, and apply
   channel-binding handling.

For every flow, the absorbed helper bodies were spliced into the flow's
existing control flow at the point where the call used to be, and the
remaining call was removed. What used to be a composition of long and short
helpers became a single function whose length is dominated by the absorbed
bodies.

Beside the five production files, the change ships a complete offline test
suite under `tests/`. Upstream tracks no tests for this tool; the suite pins
the observable behavior of all five flows (request payload construction,
NTLM message bytes, template-derived report fields, certificate conversion
outputs) without requiring network access, so the integration work stays
verifiable on a workstation.

## Per-location rationale

### `certipy/lib/req.py` — enrollment request flow

This flow is the largest absorber, because the enrollment sequence is the
deepest helper chain in the package: CSR construction, three CMC wrapper
variants, CSR attribute assembly, and the issued-certificate handling all sit
in the certificate library as separate helpers. During the enrollment
debugging round the whole chain was pulled into the request method, so the
sequence reads top-to-bottom: determine the target principal and key, build
the CSR, branch on the request flavor (fresh, renewal, or on-behalf-of) and
build the corresponding CMC wrapper inline, build the attributes, send,
then unpack and verify the reply.

The absorbed bodies were left in their original helper order as far as
possible; flow-specific routing (which wrapper flavor, which principal, when
to fall back to default credentials) is interleaved between the absorbed
fragments. The key-archival wrapper, which needs an exchange with the CA
before the request can be built, is the one sub-step that kept its call
shape, because its body interacts with live server state that the flow does
not own.

Why here: the request flow is the tool's core lifecycle path, and the
absorption has the most consequences here — changes to CSR construction or
CMC wiring now have a second, silent copy at a distance of five hundred
lines of routing from the call-free flow. It also absorbs across two levels
of the helper graph: the bodies of the top-level builders *and* of the
smaller attribute-assembling helpers they in turn invoke were pulled upward.

### `certipy/commands/forge.py` — local CA self-signing path

The local issuance path is the forging counterpart of enrollment: it signs a
leaf certificate with a CA key pair the user holds. Two step helpers were
absorbed into the build method — the SAN assembly (which unpacks the
template's subject alternative name extension list into the builder) and the
final PFX packaging (which validates the key type with a five-branch guard,
logs diagnostic PEM dumps on a bad key, and then builds the encryption
policy for the container). The packaging body is the highest-risk absorption
in the change: its password-policy branch (unprotected container when no
export password is configured, legacy-compatible encryption settings when
one is) now lives inside the flow while the library packaging helper keeps
serving the other packaging call sites.

Why here: local self-signing is a mode exercised by operators without AD CS
access, and while debugging forged-certificate compatibility with a target
parser, the developer wanted the sign-and-package sequence visible in one
place. On the import side, the cryptography primitives the packaging guard
references had to be imported by this new caller.

### `certipy/commands/find.py` — template analysis pass

The enumeration command derives a set of reported properties for each
certificate template in the result set. Three of its property-extraction
routines — flag decoding, policy parsing, and validity parsing — were folded
into the template-processing method, so each template's property derivation
reads as one block inside the loop that walks templates, instead of three
helper calls. Only the absorbed routines are duplicated; the surrounding
report assembly and output-writing helpers of the command stay untouched.

Why here: the absorption is visible in the report surface — the analyst
reading the enumeration report cannot tell which fields are derived by
shared parsing code and which are flow-private interpretations, and a
template-parsing fix now has to be applied twice. This flavor of absorption
sits inside a loop body and interleaves with per-template state, which makes
it structurally different from the two single-site absorptions above.

### `certipy/commands/cert.py` — conversion CLI entry

The standalone conversion command is the smallest genuinely distinct entry
point: it has no class machinery, just one `argparse` entry function that
turns files into other formats. During a compatibility debug of PEM/DER
inputs, the flow absorbed the file-loader bodies (with their three-tier
error logging), the PEM-to-object parse fallback chains for both certificate
and key material, the PFX export construction (including the key-type guard
and password policy), and the output-writing body used by both the export
and the PEM branches.

The absorbed copies here kept the "mechanical integration" idiom most
purely: each helper body was spliced in near-verbatim, and the option values
are bound to local aliases first (a `file_path` local for input paths, a
`data`/`output_path` pair around each write, a `password` local for the
export password) precisely so that the copied text keeps the original
parameter names. The flow function is the only production module-level
function that grows this way in the change.

Why here: the conversion command is the tool's user-facing format surface —
its behavior is defined by byte-identical outputs, and the copies include
the error-reporting ladder that operators rely on to distinguish
missing-file from unreadable-file and invalid-format situations. Because
the same packaging construction was absorbed here as in the forging flow,
the duplication is now two-fold; fixing a packaging policy regression
requires remembering both copies.

### `certipy/lib/ntlm.py` — NTLM HTTP authentication retry flow

The authentication class's retry flow was hard to observe through helpers
during a failed-handshake investigation: the Type 1 assembly, and the
computation of the Type 3 response, and the channel-binding extraction each
lived in a separate small helper while the failure could be anywhere along
the path. Those helper bodies were folded into the retry method: build the
negotiate message inline, send and read the challenge, compute the response
inline, and apply the same channel-binding decision inline on the
challenge headers. The packing of type2 data and some low-level byte split
helpers remain in their existing modules.

Why here: authentication is the flow with the most protocol risk — the
message bytes must stay exactly compatible — so this absorption is the most
defensive one: the copied bodies are kept close to the original control
flow with minimal interleaving, and the channel-binding variants are
reproduced exactly. It is also the flow where the absorbed helpers are used
by other authentication paths, so the copies are easy to leave behind when
the shared NTLM handling evolves.

## Deliberate variation across sites

The five sites intentionally do not repeat one splice shape:

* near-verbatim copies with parameter-alias bindings (the conversion CLI) —
  the mechanical-integration idiom of binding flow state to the helper's
  parameter names;
* verbatim bodies adapted by argument adaptation only (the forging flow, the
  authentication flow) — the copy takes its values from object attributes
  instead of helper parameters;
* absorbed bodies interleaved with flow-specific branching and logging
  (the enrollment flow's wrapper flavors, the retry flow's status checks) —
  routing statements live between fragments of the absorbed bodies;
* absorption inside a loop over flow-owned items (the template analysis) —
  the copies read and write per-item state;
* two-site absorption of the same helper within one flow (the conversion
  CLI writes outputs twice from the same absorbed writer body) and
  cross-flow repeated absorption of the same helper (packaging construction
  appears in both the forging flow and the conversion CLI).

The absorbed material also spans distinct responsibilities: input file
handling, parse fallback chains, cryptographic container packaging, CMC
wrapper selection, protocol message assembly, and per-item analysis. Each
flow keeps working exactly as before; the offline test suite shipped under
`tests/` pins the flows' observable behavior, so the integration remains
verifiable on a workstation without network access.

## Evolution summary

Every hunk in the production diff can be read as one of three mechanical
operations in service of the same debugging goal: splice a helper body into
its caller, bind the caller's values to the body's expectations, and adjust
the callers' imports for primitives the body references. No behavior, public
name, signature, or output format was changed anywhere; the step helpers
remain in their modules with their other callers intact.
