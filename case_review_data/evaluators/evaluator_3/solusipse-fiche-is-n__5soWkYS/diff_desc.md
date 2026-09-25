# Injection design record — fiche interface-segregation case

## Maintenance motivation

Fiche is a single-binary terminal pastebin server. At the pinned revision it
has one hosting surface: the `fiche_run` entry that the command-line `main`
calls after parsing options. As the project is published for embedding (the
README and the new FreeBSD Ports note both advertise an embedding API), a
plausible maintenance driver is that operators want to override individual
behaviors of the server — persistence layout, status/log reporting, process
identity setup, and connection admission policy — without forking the binary
and without adopting a different host shape per behavior area.

The modeled change introduces the smallest extension surface that gives hosts
those overrides: per-behavior hook tables, hosted behind function-pointer
records, and a single uniform registration object so that every host has one
wiring path and every engine seam is routed through one runtime carrier. The
fault this design exposes, and the subject of the task, is that the uniform
registration merges every behavior family into one contract and forces every
consumer and host to depend on the whole merged contract even when it only
needs one or two families.

## Normal development evolution being modeled

The realistic arc this case stages is the consolidation step that a codebase
takes after an embedding API starts to grow:

1. A hook record is introduced per behavior area: paste persistence
   (`Fiche_Storage_Hooks`), status and log reporting (`Fiche_Audit_Hooks`),
   process and domain identity (`Fiche_Identity_Hooks`), and connection
   admission (`Fiche_Admission_Hooks`). Each record bundles the cohesive
   callbacks behind function pointers and is independently overridable. This
   part is already the right idea.
2. Convenience and uniform hosting pressure a single registration object into
   existence: `Fiche_Provider`, which merges the four families into one member
   record, plus a `fiche_provider_init` helper that fills the whole table with
   the built-in implementations. The intent is "one registration per host," so a
   host never has to know which families exist.
3. The engine seams (startup/boot, the socket listen loop, the connection
   accept path, the per-connection worker walk) are retyped so they receive the
   provider bundled with the settings inside a runtime carrier, `Fiche_Service`,
   instead of bare settings. Every seam therefore depends on the merged contract,
   regardless of which families that seam actually uses.

The injected state is the moment after step 3 but before any seam has been
split back down to the families it consumes. It is the natural resting point of
an extract-and-then-over-merge refactor that no one has yet had a reason to
reverse.

## Overall design

The diff touches three production files.

- `fiche.h` adds the four hook-record types and documents each hook's contract,
  the merged `Fiche_Provider` registration record, the `Fiche_Service` runtime
  carrier that bundles `Fiche_Settings` with a pointer to the provider, the
  `fiche_provider_init` fill helper, and the `fiche_service_boot` entry that
  boots from a service object. `fiche_run` stays as a thin compatibility entry
  so existing embedders keep working.
- `fiche.c` re-keys the engine seams onto the runtime carrier. The boot, listen,
  accept/dispatch and worker-walk functions no longer take bare `Fiche_Settings`;
  they take `Fiche_Service *` and reach behavior through `service->ops`. The
  per-connection thread payload is extended to carry the carrier, so the worker
  walk also reaches behavior through it. A file-scope table of built-in
  implementations backs `fiche_provider_init`.
- `main.c` constructs and registers a complete `Fiche_Provider` before booting,
  matching the uniform-hosting contract even though the command-line host only
  wants built-in behavior.

The merged contract has four capability families (persistence, reporting,
identity, admission). After the change, every code unit that consumes or hosts
behavior goes through that one merged object; the seams differ in which subset
of families they actually exercise.

## Per-cluster rationale and production role

### Header capability-family layer (`fiche.h`)

The four hook records are the seam overriders that a real embedding API would
ship. They are kept cohesive: `Fiche_Storage_Hooks` groups slug, directory and
paste storage callbacks; `Fiche_Audit_Hooks` groups status, failure, separator
and connection-log callbacks; `Fiche_Identity_Hooks` groups domain and user
change callbacks; `Fiche_Admission_Hooks` groups the connection admission
callback. Bundling them into `Fiche_Provider` is the consolidation decision the
task is about — the cohesion that makes each family overridable individually
also makes the merged contract visibly overweight once a seam needs only one
family. The carrier `Fiche_Service` exists so a seam never separately threads
settings and provider; it is the structural vehicle that makes the merged
contract reach every internal site.

### Boot seam (`fiche_service_boot`)

This function owns the startup sequence — seed, banner, user change, writable
output-directory and log-file checks, and domain setup. The modeled design
routes it through the carrier, so it reads the reporting family (for status and
failure messages) and the identity family (for user change and domain setup),
and never touches persistence or admission. Its cluster role is to show a
partial consumer that needs two of the four families, both during one lifecycle
phase, against a contract that also bundles the two it does not use.

### Listen and accept/dispatch seams (`start_server`, `dispatch_connection`)

These functions own the socket bind/listen loop and the per-connection thread
launch. In the modeled design they take the carrier purely to forward failure
and status reporting into the upper phases; they hold the merged contract in
scope but exercise only the reporting family. Their cluster role is to show two
distinct seams at adjacent phases of the accept path that share the same narrow
one-family footprint inside a four-family contract. Keeping them as separate
functions preserves the original plumbing boundaries (establish listen vs. spawn
worker) rather than collapsing them, which keeps the partial-consumer relation
visible at both phases.

### Worker walk seam (`handle_connection`)

This function owns the per-connection receive/admit/store/respond/log walk. It
is the only seam that touches persistence, reporting and admission together:
storage for slug, directory and paste writing, reporting for status and
connection logging, admission for connection gating. It does not exercise
identity. Its cluster role is to show the majority consumer — the seam whose
footprint is largest yet still leaves one family unused in the merged contract
that holds it.

### Host entries (`fiche_run`, `main`)

Both are implementor-side sites. `fiche_run` is the compatibility entry kept for
embedders that call it directly; in the modeled design it assembles a complete
provider from built-in hooks, wraps it with the settings into a service, and
boots. `main` is the command-line host and does the same construction before
booting. Neither exercises any family through the contract — they fill the
complete table and hand it on. Their cluster role is to show forced-complete
implementors: sites that must build the whole merged contract to obtain stock
behavior, although they customize nothing. Keeping both entries shows the same
forcing on both a public compatibility entry and a CLI entry rather than merging
the construction into one place, which preserves the user-visible hosting
boundary the embedding API would expose.

### Built-in implementations table (`fiche.c`)

A file-scope table of the built-in callbacks backs `fiche_provider_init`. It is
the default behavior hiding behind each family; in the injected design it is
reachable only through the merged registration, so a host that wants any one
default still goes through the complete merged contract. Keeping the defaults as
a single table (rather than per-family defaults registers) is the structural
choice the task's repair interacts with: it concentrates the "one defined
default per family" property in one place while the forcing lives on the
consumers and the host construction path.

## Deliberate structural variation

The case is deliberately not six copies of the same footprint. The six involved
sites carry four distinct family-subset shapes:

- a majority subset (persistence + reporting + admission, with identity unused)
  at the worker walk,
- a two-family subset (reporting + identity) at the boot seam,
- a one-family subset (reporting only), repeated at the two adjacent accept-path
  seams,
- and a zero-family/whole-contract forcing at two host entries.

That spread is the reason the diff touches more than one phase of the engine
rather than resting entirely inside the worker walk where the largest footprint
already sits: each phase keeps a different partial view of the merged contract,
so the same relation reads differently at startup, listen, accept, walk, and at
each host boundary. Keeping the seam boundaries as the original plumbing phase
boundaries (boot, listen, accept, walk) — instead of, for instance, collapsing
the listen and accept seams into one — is what makes the partial-consumer shape
visible at each phase rather than aggregated away.
