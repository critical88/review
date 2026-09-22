# Injection design record — godns per-domain DNS update flow

Assigned smell category: **data clumps** (record-shaped groups of values
repeatedly passed together).

## Maintenance motivation (why this change is realistic)

godns recently grew multi-provider support: alongside the legacy global
`provider` setting, a domain entry may now name its own provider, and a
`providers` map holds each provider's credentials. That feature put
per-domain provider selection exactly where a domain's identity is needed as
a unit: the update path must know *which provider owns this domain* — not
just the global one — together with the domain's name and subdomain list, and
startup validation must check each domain entry's provider reference.

A believable developer response to that pressure is to stop threading the
whole domain entry through the update flow and instead pass the three values
the flow actually consumes as individual parameters: the domain name, the
subdomain list, and the domain's provider (which may be empty and mean "use
the global provider"). The reasoning at the time would be "the update flow
should not depend on the entire settings model just to read one entry's
fields" and "validation should validate the fields it was given, not re-read
the config". That refactor compiles, keeps every test green, and feels like a
decoupling win — while the domain's identity, which the configuration model
already models as one record, becomes a three-tuple that each new consumer
must re-declare.

## Normal development evolution modeled

1. Per-domain provider support lands; the update path now needs the domain's
   own provider, with fallback to the global provider when a domain entry
   does not name one.
2. Following up, the update-flow entry points are changed to accept the three
   consumed values explicitly instead of the domain entry, so the flow "no
   longer leans on the settings model".
3. That change forces small helper work at both directions of the flow:
   provider resolution becomes a per-domain lookup taking the two fields it
   needs (name and provider, with a fallback helper for the empty case), and
   startup domain validation is extracted into a helper taking the entry's
   fields plus the config.
4. The manager's run loop, which used to hand each domain entry to the update
   flow, destructures the entry into locals first; the loop-lifecycle tests'
   call sites are adapted to compile.

This is a single understandable chain of edits one developer could plausibly
make across one afternoon — not unrelated changes bolted together.

## Overall design

The domain entry record (`domain name`, `subdomain list`, optional
`provider`) from the configuration model is consumed by the update flow as
three separately declared parameters — `string`, `[]string`, `string` — and
that loose trio now recurs through the update flow's signatures:

- the per-domain background update loop entry,
- the one-shot update entry,
- the DNS record-update step,
- a provider-resolution helper taking a two-field subset of the trio,
- and a per-domain startup validation helper.

The orchestrating run loop destructures each configured domain entry into
name/subdomain/provider locals before handing them on, and a single-field
fallback helper resolves the empty provider field to the configured global
provider. The configuration model itself, the provider interface, and the
notification/webhook delivery paths are untouched: the evolution is contained
to the update flow, its orchestration fan-out, and startup domain
validation — the places that genuinely needed per-domain provider knowledge.

Production roles of the touched flow, for context:

- `internal/handler/handler.go` — the per-domain update engine: periodic
  loop, one-shot update with IP caching and fail-fast behavior, provider
  dispatch and record update, success notification.
- `internal/manager/dns_manager.go` — lifecycle orchestration: watches
  config, restarts, and fans each domain out to a background loop or a
  synchronous run.
- `internal/utils/settings.go` — startup validation of the settings,
  including per-domain validation in multi-provider mode.
- `internal/handler/handler_test.go` — concurrency tests guarding the update
  loop's cancellation and concurrent-shutdown behavior.

## Per-cluster changes

### 1. Update-flow signatures (`internal/handler/handler.go`)

**What changed.** The three update-flow methods that previously received a
domain entry now declare its fields individually:

- the background loop entry takes the loop context plus domain name,
  subdomain list, and provider name, and still runs one update immediately,
  then on the configured interval, until the context is cancelled;
- the one-shot update entry takes the same three values and keeps its
  IP-caching and RunOnce fail-fast behavior;
- the DNS record-update step takes the three values plus the current IP,
  resolves the provider through the per-domain lookup, and builds the
  success message with the provider that handled the update.

Provider resolution was reshaped to match: the per-domain lookup now takes
the domain name together with the provider name, resolves the effective
provider (falling back to the configured global provider when the entry has
none) before looking the provider up in the multi-provider map or falling
back to the legacy single provider, and returns both the provider and the
effective provider name used by the notification.

**Why this site and shape.** This file is the only place that performs DNS
updates; the three values are precisely what it consumes, so a developer
"decoupling" it from the settings model would land the decomposition here
first. Each signature keeps its original control flow, caching, fallback
ordering, and error semantics — the change is limited to how the domain's
identity and provider reach the method.

**Production role.** The hot path: every configured domain is updated by
these methods on every tick; the notification names the provider that handled
each updated domain.

### 2. Provider-name fallback helper (`internal/handler/handler.go`)

**What changed.** A small package-private helper resolves empty provider
names to the configured global provider. It exists because once the provider
travels as an individual field, the "domain-specific or global" question has
to be answered somewhere, and the natural quick answer is a one-field helper
next to the lookup.

**Why this shape.** It models the texture that develops around decomposed
records: the record used to answer "which provider?" in one place; now the
scattered field needs its own micro-policy function.

**Production role.** Ensures per-domain provider selection still prefers the
domain's own provider and falls back to the global one.

### 3. Orchestration fan-out (`internal/manager/dns_manager.go`)

**What changed.** The run loop, which previously handed each domain entry as
a unit to the update flow, now destructures every entry into three locals
(domain name, subdomains, provider) and passes them — synchronously in
RunOnce mode, or to a background goroutine for the periodic mode. Everything
else (restart, watch, RunOnce exit codes) is untouched.

**Why this site.** This is where domain entries enter the update flow. Once
the flow's signatures take fields, the entry must be dissolved into locals
here — the natural seam for the destructure-and-hand-off edit.

**Production role.** Lifecycle orchestration: choosing the loop vs one-shot
path per configuration, and spawning the per-domain goroutines that keep
running until the manager's context is cancelled.

### 4. Startup domain validation (`internal/utils/settings.go`)

**What changed.** In multi-provider mode, the per-entry validation loop was
extracted into a helper that receives the entry's fields — domain name,
subdomain list, provider name — together with the settings, and re-implements
the existing checks in field form: non-empty name, non-empty subdomains, the
effective provider must exist (domain-specific provider provided it names
one, otherwise the global provider), and a domain-specific provider must be
present in the configured providers map. Validation outcomes and error
messages for misconfiguration are unchanged.

**Why this site and shape.** Validation already ran per entry; extracting a
per-entry helper is a normal readability follow-up once the entry is
consumed "as fields" elsewhere in the codebase — the developer copies the
newly fashionable field-based style into validation rather than keeping the
record. Keeping the outer multi-provider and single-provider structure
untouched limits the blast radius of the refactor, as a real developer would.

**Production role.** Startup protection: rejects empty domain names,
empty subdomains, and domains whose named provider is not configured,
before the manager starts any update loop.

### 5. Test call-site adaptation (`internal/handler/handler_test.go`)

**What changed.** The two loop-lifecycle tests construct their own
name/subdomain/provider values and call the loop entry through the new
field-based signature. Their assertions and lifecycle coverage (cancellation
return, concurrent shutdown) are unchanged.

**Why.** Mechanical compile-fix of the tests that sit on top of the changed
signatures — a developer landing step 4 of the evolution. No test logic was
added or removed.

**Production role.** Keeps guarding that per-domain loops exit on context
cancellation and that concurrent loops shut down together.

## Scope boundary (deliberate)

The provider interface (`UpdateIP(domainName, subdomainName, ip)` shared by
all DNS backends), the webhook and notification senders, the
configuration-domain record type and its accessor, and the provider factory
were left as-is: the evolution being modeled is about how the *update flow*
consumes domain entries, not a redesign of provider boundaries or the
configuration model.
