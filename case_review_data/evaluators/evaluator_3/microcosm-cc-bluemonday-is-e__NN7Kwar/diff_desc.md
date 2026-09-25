# Injection design record — policy-definer seam in bluemonday

## Maintenance motivation

bluemonday lets callers build sanitization policies through a long chain of
exported declaration methods on `*Policy` (element and attribute whitelists,
URL schemes and custom URL policies, link hardening, output toggles) and then
run those policies over untrusted input (`Sanitize`, `SanitizeBytes`,
`SanitizeReader`, `SanitizeReaderToWriter`). Three groups of code currently
retype that surface independently: the convenience rule blocks in `helpers.go`
each hard-code `*Policy`; the built-in UGC profile in `policies.go` assembles
its rules on the concrete policy; and both command-line tools declare their
extras directly on the policy they obtain from `UGCPolicy()`.

Two maintenance pressures make a shared declaration seam attractive rather
than the concrete type:

- Reusable rule blocks ("standard URL rules", "standard attribute rules",
  "list rules", "table rules", "image rules") are wanted by profile
  constructors that do not necessarily own a raw `*Policy` — wrappers, guarded
  or audited profiles — and writing every block against the concrete type
  means a guarded or auditing profile cannot reuse anything.
- The user generated content profile has a standing requirement that the
  profile's rules be assembled through one choke point so that declarations
  the profile does not support (unsafe parsing, comments, data attributes,
  raw CSS, iframes, custom URL rewriting) are refused in one place instead of
  being overlooked at each construction site.

The change therefore introduces one exported seam type — `PolicyDefiner` —
that captures the full declaration-and-run surface of a policy, and routes
every participating caller through it.

## Modeled evolution

This models a normal single-maintainer feature step, the kind that lands as
one coherent commit series:

1. Introduce the seam type in the core (`policy.go`) so wrappers and guarded
   profiles can be handed to anything that builds policies, replacing the
   per-caller retyping that `helpers.go`, `policies.go` and the two ports
   each did on their own.
2. Convert the package's own rule blocks to the seam so future profile
   constructors can reuse them wall-to-wall (the obvious way to consume the
   new seam once it exists).
3. Stand the UGC profile's assembly on the seam through a conformance guard
   that enforces the profile's standards for every declaration.
4. Give both command-line tools the same treatment for their extracted
   rule-declaration and streaming functions, so the ports program against the
   library's seam rather than a concrete policy.

Each step is the kind of follow-on a developer makes once a shared surface
exists: once any seam type is published, consumers gravitate to it because it
is the documented way to stay forward-compatible.

## Overall design

- `policy.go`: new exported interface `PolicyDefiner` documenting the single
  seam through which anything defines a policy's rules and runs it. Its
  method set mirrors the concrete policy's exported definer surface exactly
  so `*Policy` satisfies it implicitly, with the same signatures the public
  API already publishes.
- `helpers.go`: the eight public block helpers (`AllowStandardURLs`,
  `AllowStandardAttributes`, `AllowStyling`, `AllowImages`,
  `AllowDataURIImages`, `AllowLists`, `AllowTables`, `AllowIFrames`) become
  thin wrappers over new `declareX` package functions that take a
  `PolicyDefiner`. The declaration statements inside each block are moved
  verbatim; the wrapper keeps the existing public API and behavior.
- `policies.go`: new unexported `ugcConformanceGuard` implementing
  `PolicyDefiner` around a `*Policy`. It forwards the capabilities the UGC
  profile supports — element and attribute declarations, the standard
  schemes, parseable and relative URL rules, nofollow hardening, and running
  the policy — and refuses the rest by returning the policy unchanged,
  returning an inert builder, or doing nothing, each with the profile rule
  that motivates the refusal. `UGCPolicy()` assembles the profile by handing
  the guard to the rule blocks and to its own hand declarations.
- `cmd/sanitise_ugc/main.go`: the stdin-to-stdout runner is extracted into
  `sanitizeInput(pd bluemonday.PolicyDefiner, in io.Reader, out io.Writer)`,
  and the tool keeps its three extra hardening declarations on the concrete
  policy it already holds.
- `cmd/sanitise_html_email/main.go`: the email-specific rule block is
  extracted into `declareEmailRules(pd bluemonday.PolicyDefiner)` and the
  streaming into `sanitizeEmailInput(...)`, while the two ports-only helpers
  (`AllowStyling`, `AllowDataURIImages`) stay on the concrete policy because
  they are not part of the seam surface.

## Rationale per changed location

### `policy.go` — the seam declaration

The concrete policy's definer surface is already public API; collecting it
into an interface changes nothing about what the methods mean and nothing
can break, because Go satisfies interfaces implicitly. The seam is declared
next to `urlRewriter` in the core so that anyone reading the policy type
finds the seam documented in the same place as the custom policy types. Its
groups mirror how the toolkit is documented (run, elements, attributes,
styles, URLs, hardening, output handling) so the seam matches the mental model
users already have from the method names themselves. The production role: the
one type that represents "something that can define and run a policy",
independent of the concrete storage.

### `helpers.go` — the eight rule blocks

Each block is a group of declarations a caller might want to apply to
something that is a policy definer rather than the concrete policy: a guarded
profile, an audited wrapper, a test double. Once the seam exists in the core,
programming the blocks against the seam is the natural form: the block
bodies keep their exact declaration statements (same regexes, same element
lists, same call chains), so behavior is the moving-of-verbatim-code only.
The wrappers stay one-line forwarding methods so no existing caller or doc
example changes. The `AllowIFrames` block also gains its missing doc comment,
since becoming a wrapper is precisely when the exported behavior deserves a
sentence of documentation.

### `policies.go` — guard and profile assembly

The UGC profile is the toolkit's security-critical showcase: it must not
allow unsafe parsing, comments, data attributes, raw CSS, iframes, or custom
URL rewriting, and it must apply parseable-URL, scheme, and nofollow
discipline to everything. Assembling the profile through a guard that
implements the full seam concentrates that enforcement in one type instead of
scattering "this profile does not support X" comments across the profile
body. The refusals are deliberately written as cheap inert bodies (return the
policy unchanged, return a builder that binds nothing, empty body) with a
comment naming the profile rule; the capabilities the profile supports are
forwarded to the concrete policy, and the profile's own hand declarations go
through the guard before reaching the policy. The production role: the
single point through which every UGC rule declaration passes, for callers
that compose profiles out of shared blocks and their own declarations.

### `cmd/sanitise_ugc/main.go` — UGC port

The tool is a Unix-style filter: read stdin, sanitize, write stdout. Once it
is no longer considered a policy constructor, only its runner behavior
matters, and extracting `sanitizeInput` gives it the seam type so it is typed
against what it does. Its three extra hardening flags (an extra nofollow
requirement, fully-qualified nofollow, external links into new tabs) are
policy-construction decisions the tool makes about its own policy before
anything else happens, and they keep operating on the concrete policy the
tool already holds. Production role: unchanged filter behavior with the
runner expressed through the seam.

### `cmd/sanitise_html_email/main.go` — email port

Email processing is the port that needs the most extra rules (document
structure elements, deprecated styling attributes, button and style tags,
colors) in addition to the UGC base. The declarations group naturally into
one `declareEmailRules` block — a local sibling of the library's own rule
blocks — which is also where the tool's nofollow and new-tab hardening
choices live. Separating the email rule block from the streaming function
(`sanitizeEmailInput`) keeps declaration and execution readable on their own.
The two ports-only helpers stay on the concrete policy because the seam
deliberately carries only the shared definer surface. Production role: the
email-specific rule book, expressed once instead of being interleaved with
I/O.

## Structural variation across locations

The change is deliberately not one repeated edit:

- one type declaration in the core (pure interface, no behavior);
- eight helper wrappers plus eight extracted block functions of very
  different sizes (from a one-statement styling block to the twenty-line
  table block), each moved verbatim behind a seam-typed parameter;
- an implementing type carrying twenty-eight methods split between forwarded
  capabilities and inert refusals, with a profile-assembly body re-pointed
  to route through it;
- one command-line port gaining a seam-typed runner while keeping its final
  hardening flags concrete;
- one command-line port gaining both a seam-typed local rule block and a
  seam-typed streaming function while keeping two ports-only helpers
  concrete.

## Compatibility considerations

All exported names, signatures and method chains are unchanged; `*Policy`
satisfies the new seam implicitly. The extracted block bodies are the same
declaration statements as before, and the tool runner behavior (stdin to
stdout, same error reporting, same log on read failure) is preserved. The
email port preserves its original declaration order so the composition of
its rules is unchanged.
