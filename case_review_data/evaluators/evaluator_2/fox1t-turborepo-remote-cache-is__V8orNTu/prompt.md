# Task: rework the overloaded storage-provider contract (interface segregation)

## Maintainer observation

We recently widened the shared storage-provider contract in this cache
server so that every backend speaks the same full artifact-lifecycle
vocabulary. The intent was to prepare the ground for artifact-retention work
and to stop callers from having to treat backends differently. Instead we now
have a contract where:

- a backend must implement capabilities it cannot genuinely support or that
  no runtime path consumes for that backend (for instance, publish/delete
  surface that only one backend's flow ever reaches, and
  maintenance-style members such as listing or timestamp reporting that
  nothing consumes yet);
- the burden looks uniform but the runtime is not: the artifact upload flow
  selects the publishing branch by comparing provider identities, so the
  implementations carried by three of the four backends can never execute;
- adding a new backend means implementing an entire lifecycle before the
  backend can participate at all, even when the new backend only offers plain
  blob storage.

This is an interface segregation problem: implementations and consumers are
being forced to depend on capabilities they do not use. The defining issue is
not the interface's size alone, but that the unwanted obligation is baked
into the shared contract itself.

## Scope boundary

The storage layer under `src/plugins/remote-cache/storage`: the shared
`StorageProvider` contract, the surface its four backends (local filesystem,
S3/minio, Google Cloud Storage, Azure Blob Storage) are required to carry,
and the places in the storage layer that decide which capability actually
runs for a given backend. Nothing else is in scope unless the contract change
genuinely requires touching it.

## Desired outcome

Apply interface segregation to this contract:

- Split the overloaded contract into focused, cohesive capability groups —
  or another sound segregation of the same responsibility (for example, a
  minimal universal contract plus explicit optional capabilities), as long as
  no implementation is forced to depend on members it does not support or
  use.
- Each backend ends up carrying only contract surface it genuinely supports
  and that some real runtime path can consume for that backend.
- Publish/cleanup decisions in the storage layer resolve through a backend's
  actual capability, not through a broad uniform obligation paired with
  identity special-casing.
- Speculative or unwired capability surface introduced by the widening —
  implementations as well as contract declarations — is removed or moved
  behind an explicit, segregated capability boundary. Do not leave unused
  per-backend plumbing behind.
- The refactored code remains strictly typed (no `any` escapes or assertion
  casts to defeat the narrowed contract) and biome-clean.

## Behavior and compatibility boundary (must remain stable)

- Every backend's observable behavior is exactly what it is today: the three
  object stores stream uploads directly to the final key; the filesystem
  backend writes to a temp sibling key and publishes it atomically so a
  failed or oversized upload never exposes a truncated artifact at the
  final path and a previously cached artifact survives a failed overwrite.
- A failed upload still cleans up its temporary key without masking the
  original failure.
- The HTTP API, response codes, and the 404-cache-miss versus 5xx-backend-
  failure mapping are untouched; the route layer needs no changes.
- The complete existing test suite (212 cases, including the per-backend
  upload/download tests, body-limit enforcement, and cache-miss behavior)
  passes without any test edits; treat a need to change tests as a signal
  that behavior changed.

## Notes for investigation

- The failure-cleanup path and the publish decision are coupled: whichever
  way you re-segregate capabilities, a backend without a given capability
  must still behave as it does now on both the success and failure paths.
- The local stale-artifact cleanup feature is intentionally local-only today
  and explicitly declines other providers; its handling pattern is a useful
  contrast to the current state of the shared contract.
- This is a genuine refactor: no behavior is meant to improve or change, and
  the finished state should look like the kind of contract design a careful
  maintainer would have written for four such backends.
