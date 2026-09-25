# Injection design record: consolidating the package installation subsystem

## Maintenance motivation

The package installation subsystem is the part of aqua that downloads release
assets, verifies them, extracts them, and installs tools from Go module sources,
local Go sources, or Rust crates. It also installs the verification binaries
(cosign, slsa-verifier, minisign, GitHub CLI) that some verification schemes need,
as soon as they are first required.

The change modeled here is the kind of consolidation that happens during a
feature push against a deadline. Engineers working on this subsystem keep having
to answer cross-cutting questions uniformly: which external command executor runs
`go install`, `go build` and `cargo`; how verification tools get installed before
an asset is verified; which policy object (asset configuration or checksum
configuration) feeds a verifier. Repeating those answers across a dozen small
types gets tedious, and reviewers had repeatedly asked for "one place" that
coordinates the whole installation story. Modeling that pressure, the change
routes every installation mode and every verification family through the main
installation coordinator: the coordinator becomes the single place where command
execution, verification, tool provisioning and policy selection are decided,
which is attractive in the short term because every new mode or scheme lands in
one familiar type.

## Modeled development evolution

The diff models how such an omnibus grows in normal increments rather than in
one designed step:

1. Source-install helpers first (they are the simplest): the `go install`,
   `go build` and `cargo` implementations move onto the coordinator, and the
   helper types keep a reference back to the coordinator so their existing
   public methods still work for every caller.
2. The constructor then adopts the concrete helper implementations with type
   assertions, so mock and third-party implementations keep their own behavior
   while the production wiring is rebound.
3. Artifact verification follows the same recipe, family by family: each
   verifier keeps `Enabled` (cheap policy lookup) and forwards `Verify` into the
   coordinator, where a per-family method reads the verifier's own fields to run
   the same steps.
4. The dedicated verification-tool installation step moves the same way, since
   it already needed coordinator state.
5. Finally the verifier-policy factory that both the download path and the
   checksum-file path use is unified into a single coordinator method with a
   mode flag, which is the natural cleanup once the coordinator owns all
   verifier construction knowledge.

Each increment is individually reviewable and keeps the package compiling, which
is exactly how responsibility drift accumulates in real codebases: no single
review sees the destination, only a plausible next step.

## Overall design

The final shape is a hub type in the package installation subsystem that holds
the implementation bodies for

- external command execution for the three language-source install modes
  (`go install`, `go build`, `cargo install`), including cargo argument
  construction,
- the four downloaded-asset verification families (Cosign, SLSA provenance,
  minisign, GitHub Artifact Attestations),
- installation of the verification tools those families depend on,
- construction of the file verifier set for both the asset and checksum-file
  policy branches,

while the previous owner types remain in place with their external method
signatures intact, each holding a coordinator reference to route calls into the
hub. The coordinator also takes over the process executor field previously owned
by the source-install helpers, so command execution state lives in one place.

The design deliberately keeps all public entry points, construction wiring and
observable messages unchanged, so the change is purely internal re-ownership:
request handling flows through the same types as before, but the bodies now live
on the coordinator.

## Per-cluster rationale

### Language-source installers (go_install, go_build, cargo)

*What changed.* The bodies of the three `Install` implementations moved onto the
coordinator as `runGoInstall`, `runGoBuild` and `runCargoInstall`, with the cargo
argument builder following as a coordinator method. Each helper type kept its
`Install` method and gained a reference field to the coordinator; the helpers now
only delegate. The coordinator gained the executor field the helpers used to own.

*Why this site and shape.* These three helpers are the subsystem's most self-
contained units, all shaped the same way: build an `exec.Cmd`, hand it to an
executor, wrap the error. That uniformity is what makes a consolidated
implementation attractive to a maintainer trying to guarantee identical command
execution behavior across modes, and it is why they make the first natural
cluster: one reviewable increment proves the coordinator-reference pattern, and
the two remaining modes are then "obvious follow-ups" of the same kind. Taking
over the executor as coordinator state rationalizes the split-brain where three
helpers each held an executor while the coordinator, which also runs code
paths that conceptually belong to command execution, had none.

*Production role.* Every tool installed from a Go module path, from a local Go
source tree, or from a crate name flows through these entry points, under
`GOBIN`, build output and crate-directory semantics that the moved bodies
preserve verbatim.

### Artifact signature verifiers (verify_slsa, verify_cosign, verify_minisign, verify_github_artifact_attestation)

*What changed.* Each of the four verifier families kept its `Enabled` method and
its data fields, but its `Verify` method now forwards to a per-family coordinator
method that performs the verification using the verifier's own fields plus the
verification client read from coordinator state. The helpers' per-tool installer
fields were renamed (for example `installer` to `toolInstaller` for the Cosign,
SLSA and minisign helpers) because the field name `installer` was repurposed for
the new coordinator reference; the attachment verifier kept its existing
`ghInstaller` name since that was already specific enough. The attachment
verifier's verification client field moved to coordinator state only.

*Why this site and shape.* All four verifiers share an identical operational
shape — ensure the verification tool is installed, template the asset, call the
verification client — which invites one shared implementation site for
consistency, uniform logging and (in a maintainer's mind) a future "add a scheme
here" shortcut. Keeping `Enabled` on the helpers preserves the cheap policy
lookup the download loop relies on before deciding to verify anything, while
forwarding only the substantive `Verify` bodies minimizes churn per review. These
four are the subsystem's clearest instance of a repeated pattern, so once one
family's body moves, the identical shape of its siblings makes not moving them
the inconsistent choice.

*Production role.* These families guard every downloaded asset and, for three of
them, checksum file content as well: provenance, signature and attestation
checks that are the security-relevant core of the installation path.

### Verifier policy factory (checksum.go, download.go)

*What changed.* The two sites that assemble a file verifier slice — the one used
when verifying a downloaded asset, and the one used when verifying downloaded
checksum-file content — were folded into a single coordinator method that takes
a mode flag. The mode flag selects whether policies are read from the asset
configuration or from the checksum configuration, and whether the provenance
verifier participates (it does not for checksum files, which carry no
provenance). The download path now obtains its verifier set from that method.

*Why this site and shape.* Once the coordinator owns per-family verification
bodies and per-family verifier construction, the remaining duplication — two
near-identical literal lists differing by policy source — reads as obvious
cleanup: one method, one flag. A mode-flagged factory is also the shape a
maintainer reaches for when asked to make the two branches "consistent" while
keeping their documented differences.

*Production role.* This factory decides, for every download, which verification
schemes apply and with which configuration — the policy decision point for both
release assets and their checksum files.

### Dedicated verification-tool installation (dedicated_installer.go)

*What changed.* The body of the per-tool install method moved onto the
coordinator; the dedicated installer type keeps the mutex, the package accessor
and the checksum store it owns, and its unchanged method now delegates to the
coordinator, which reads those per-tool fields.

*Why this site and shape.* This step sits between two worlds: it must serialize
per tool and needs the tool's package definition, but its implementation calls
the general package installation entry point on the coordinator. Consolidating
the body onto the coordinator is the natural continuation once verification
bodies already live there — the maintainer modeling here is "the coordinator
already installs everything, so let it own tool installation outright."

*Production role.* First-use provisioning of cosign, slsa-verifier, minisign and
the GitHub CLI, each serialized against concurrent installs of the same tool and
guarded by version-constraint and environment support checks.

### Constructor wiring (installer.go)

*What changed.* The constructor gained a tail call that binds the concrete
source-install helper implementations to the new coordinator: for each of the
three helpers, if the implementation is the package's own concrete type, its
coordinator reference is pointed at the coordinator being constructed and the
coordinator adopts the helper's executor. Non-concrete implementations (mocks and
third-party substitutes) are deliberately left untouched.

*Why this site and shape.* The helpers need a coordinator reference from the
moment their bodies move, and the production wiring is where wire-generated
construction hands concrete implementations to the package. Type assertions keep
the constructor's parameter list and the wire path unchanged — no API break —
while mock installers used in-shadow by other callers keep their self-contained
behavior, which matches how the package's own tests exercise the constructors.
Adopting the executor here (rather than adding a constructor parameter) avoids
growing the already wide parameter list that the wire layer generates a second
time.

*Production role.* All production code paths construct the coordinator through
this constructor, so every real installation — CLI, and the nested coordinators
built for the four verification tools — runs through the rebinding.

## Behavior-preservation considerations

The bodies were moved verbatim wherever possible, with only field-receiver
spelling adjusted (`di.installer.runtime` becomes coordinator `runtime`, the
helper's verification client becomes the coordinator's field). The constructor
performs its rebinding after the nested verification-tool coordinators are built,
so the shared helper implementations still end up bound to the outermost
coordinator, matching the prior observable behavior for every production path.
Verification order, policy sources, host-platform support checks, command
argument order, cleanup of partially installed crates, per-tool serialization,
log and error message texts, and enablement rules are all carried over
unchanged, with the package's existing construction sites and call points serving
as the specification for the unchanged observable surface.
