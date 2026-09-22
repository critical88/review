# Injection design record — turborepo-remote-cache (interface segregation candidate)

## Motivation

The cache server stores artifacts through four interchangeable backends: the
local filesystem, S3 (including minio endpoints), Google Cloud Storage, and
Azure Blob Storage. All four are reached through one `StorageProvider`
contract inside `src/plugins/remote-cache/storage/`, which until now followed
the abstract-blob-store shape (`exists`, `createReadStream`,
`createWriteStream`) plus two *optional* capabilities — `promote` (atomic
publish of a temp-written file; a same-filesystem `rename` only the local
backend can do cheaply) and `remove`.

The modeled maintenance story: an artifact-retention workstream needs
team-level artifact listings and per-artifact timestamps to age cache
entries, and the upload flow needs a uniform way to talk about the full
artifact lifecycle across backends. The natural first step a team takes in
that situation is to widen the shared contract once, so that every backend
speaks the same lifecycle vocabulary and no caller ever has to
feature-detect a backend. That step is what this change records: the
contract grows from "blob operations" to "whole artifact lifecycle", and the
four backends are brought up to it, each in its own SDK idiom.

## Development evolution being modeled

This is the classic way a fat contract appears in a storage-layer codebase:
not as one deliberate design decision, but as a "make everything uniform
first" preparation commit for planned maintenance work. The publish/delete
capabilities already existed for one backend; unifying the contract then
obliges the other backends to grow implementations of capabilities they had
previously been explicitly allowed to skip. Keeping behavior stable — the
object stores must not start server-copying payloads on every upload — then
pushes the upload flow into provider-identity branching at the point where
the new contract surface would otherwise change what actually runs.

## Overall change design

Five files in the storage layer participate, in three roles:

1. **Contract** — `src/plugins/remote-cache/storage/index.ts` widens
   `StorageProvider` so the whole artifact lifecycle (publish after write,
   delete, list a team's artifacts, report an artifact's timestamp) is part
   of the uniform provider contract instead of two optional members.
2. **Implementations** — each backend factory gains the widened surface,
   written in that backend's native SDK idiom so each one reads as the
   obvious local way to satisfy the contract.
3. **Consumption site** — the upload flow in the same directory keeps every
   backend's runtime behavior exactly as today and gains a
   provider-identity comparison at the publish decision, mirroring the form
   of the existing `cleanStaleArtifacts` local-only guard. The failed-upload
   cleanup helper drops its capability check because the contract now
   guarantees the member.

Nothing outside the storage layer changes: no route, no signature-tag flow,
no environment parsing, no test expectations.

## Cluster-by-cluster rationale

### 1. The shared provider contract — `storage/index.ts`

The interface is the single place where the uniform-lifecycle vocabulary is
declared, so the whole migration is anchored here. `promote` and `remove`
move from optional, capability-specific members to required universal
members, and two new lifecycle members join them: a team-scoped listing
(`listArtifacts`) and a per-artifact timestamp (`getModifiedTime`). The
docblock rationale is rewritten from "providers that can promote cheaply
opt in" to "every backend exposes the whole lifecycle, so callers never
feature-detect". This is the production role the contract serves for the
planned retention pipeline, and it is also the point where the cost lands:
a new backend can no longer implement just blob storage; it must supply
deleted-listing, timestamp, publish and delete plumbing before it can
participate at all.

### 2. The upload flow and its cleanup helper — `storage/index.ts`

`createCachedArtifact` decides per request how an artifact lands in the
backend. For the local filesystem it streams into a temp sibling key and
publishes it atomically (mid-upload failures must never expose a truncated
artifact at the final path); for the object stores it streams directly to
the final key, because promoting there would mean a server-side copy — or
worse, a client re-transfer — of every artifact. Under the widened contract
the flow keeps that per-backend behavior: the publish branch is selected with
a `STORAGE_PROVIDERS.LOCAL` identity comparison, in the same shape as the
existing `cleanStaleArtifacts` guard two functions below it. The
`removeQuietly` helper that cleans up a failed upload's temp key loses its
`if (!remove) return` guard: under a contract that guarantees the member,
guarding is dead weight, and its removal is a faithful consequence of the
new contract rather than an independent edit.

### 3. Local filesystem backend — `storage/local.ts`

Already the only implementer of `promote`/`remove`, so it only grows the two
maintenance members. Listing is a raw `fsPromises.readdir` of the team
directory — the pipelines are expected to decide what to skip — and the
timestamp member reports `atimeMs`, matching what "age cache entries"
means for a filesystem cache that is both written and read on the same
volume. Both reuse the module's existing native-path mapping so the same
`toNativePath` rule keeps POSIX keys and Windows paths from leaking into
each other. This backend also shows the relation's sharpest edge: its
publish/delete implementations are real, consumed code; the same members on
the three object stores are its forced twins.

### 4. S3 and minio — `storage/s3.ts`

The AWS SDK backend is the largest contributor because its lifecycle words
are individual commands: publish is `CopyObjectCommand` followed by
`DeleteObjectCommand` of the source key, delete is `DeleteObjectCommand`,
listing is `ListObjectsV2Command` under the team as key prefix, and the
timestamp member is `HeadObjectCommand`'s `LastModified`. S3 has no
directories, so "stored directly under a team" becomes "sharing a common
key prefix", the same flattening the module already documents for its 403
cache-miss behavior. The async/await-plus-try/catch style matches the
file's existing `exists` implementation, so each new member reads as a
sibling of what was already there.

### 5. Google Cloud Storage — `storage/google-cloud-storage.ts`

This SDK prefers callbacks for operations and promises for metadata, and
the additions follow that split rather than inventing a uniform style:
publish uses `turboBucket.file(...).move(...)` (the SDK's server-side
rename), delete uses `file(...).delete(...)`, listing uses
`bucket.getFiles` with a prefix, and the timestamp member reads the object's
`updated` metadata (GCS keeps no access time). Each is two or three lines,
the way small SDK-wrapping members look in a thin adapter file.

### 6. Azure Blob Storage — `storage/azure-blob-storage.ts`

Blob storage has no server-side cross-blob rename, so publish is the
platform's copy-and-delete idiom: `beginCopyFromURL` plus a poller
(`pollUntilDone`) and a source delete, wrapped in the same promise/async
shape the file already uses for downloads and upload aborts. Listing maps
cleanly onto `listBlobsByHierarchy` under a team prefix (a flat blob
namespace is exactly a hierarchy of depth one), and the timestamp member
reports `lastModified` — the service's closest equivalent of an access
time.

## Scope deliberately left alone

The route layer, the fastify decorator, tag read/write helpers, env
parsing, and the local stale-artifact cleanup module keep their existing
shape: the retention work this change prepares is not yet wired to any of
them, and nothing in the widened contract forces them to change to keep
compiling or behaving. Whether the resulting configuration of contract,
implementations, and dispatch is a healthy arrangement or one that should be
reworked is left as an open question for independent assessment; this record
describes what was built and why each site took the form it did.
