# Injection design record

## Maintainer motivation

aqua keeps the packages it ships for its own use — the `aqua-proxy`
companion that stands in for executables on Windows, and `aqua` itself when
aqua self-installs or self-updates — separate from the packages it manages for
users. Those first-party packages live under the aqua root's *internal*
package tree rather than the public `pkgs` tree, so garbage collection and
registry-driven behavior never treat aqua's own machinery as ordinary
user-managed content. The location of that internal tree is a cross-cutting
convention: several unrelated flows need it at different moments — when
provisioning the proxy during `aqua install`, when creating or re-creating
Windows bin links, when aqua updates itself, when `aqua which`/`aqua exec`
look up an installed executable, and when the last-used timestamp of a package
is recorded for garbage collection.

The normal development evolution modeled here is a familiar one: a series of
small "reliability" patches, each shipped for a defensible local reason.
Registry content is user-controlled in aqua, and a customized registry can
redefine how a package — including one of aqua's own — is described. A
contributor who worries that shared path resolution depends on that
registry-driven description takes the apparently safe route for each affected
flow: stop asking the shared path helper for aqua's own packages; the flow
knows where its own tool lives, so compose that path inline from the fixed
repo coordinates and the rendered asset name. Every individual patch is small,
locally coherent, and reads like progress: the flow becomes self-reliant.
Nothing in any single patch announces a design change. What accumulates,
patch after patch, is the same location convention hand-written again and
again in flows that otherwise have nothing in common — so that the next
conceptual change (relocating the internal tree, adding another bundled tool,
changing the asset or version convention) silently requires finding and
editing every copy in step. That accumulated state is what this diff
reconstructs.

## Overall design

One conceptual responsibility — knowing where aqua's own packages are
installed — is re-implemented privately in seven flows across three
packages. The clean state defines the internal layout in one place; after
this diff that exclusivity is gone: each flow either hard-codes its own copy
of the layout (the flows that always deal with a fixed first-party tool) or
carries a local first-party identity test plus its own copy of the layout
formula (the flows that serve arbitrary packages). The shared resolution
remains available and continues to answer for regular packages, which is
exactly why each scattered copy looks harmless in review: the fallback still
exists, the branch never removes anything. Rendered asset names are still
obtained from the shared rendering helper everywhere, so no fragment even
duplicates templating — each fragment only duplicates the *decision*.

## Cluster-by-cluster rationale

### Proxy provisioning — `pkg/installpackage/proxy.go` (`Installer.InstallProxy`)

What changed: the flow that installs aqua's companion proxy no longer obtains
the download destination through the shared package-path resolution; it
composes the destination itself from the root directory, the fixed
`github_release` type, the `github.com/aquaproj/aqua-proxy` coordinates, the
pinned proxy version and the rendered asset name.

Why this site and shape: provisioning the proxy is the first internal-package
flow any skeptic would harden, because a broken proxy destination breaks every
subsequent Windows invocation. Since the flow constructs its own proxy package
definition a few lines above, deriving the destination from the same fixed
coordinates (`pkgTypeGitHubRelease`, `proxyName`) looks locally consistent,
and the fragment reads as a natural continuation of the code around it. The
already-rendered `assetName` above is reused, keeping the fragment to a single
join.

Production role: this is the proxy bootstrap inside `aqua install`; the
destination directory is also what the install step later treats as the
package content root.

### Windows hard-link creation — `pkg/installpackage/link.go` (`Installer.createLinks`)

What changed: when preparing command links on Windows, the flow computes the
aqua-proxy hard-link target by joining the root directory with the internal
layout segments plus the `aqua-proxy.exe` file name, instead of resolving the
proxy package directory through the shared helper and appending the file
name.

Why this site and shape: this fragment required an asset name that the flow
previously never needed, so the change brings its own `RenderAsset` call and
keeps the original diagnostic message and failure flagging behavior around
it, mirroring how a careful patch preserves a function's error contract. The
join stays one expression; the trailing `aqua-proxy.exe` join from the clean
code is kept verbatim so the produced string is unchanged.

Production role: every command link on Windows is a hard link to the single
proxy binary; this is the path that makes `aqua`-installed tools executable on
Windows without per-tool copies.

### Link re-creation during aqua's own update — `pkg/installpackage/link.go` (`Installer.recreateHardLinks`)

What changed: the maintenance pass that re-points existing bin entries at the
proxy binary now derives the proxy file location itself — rendering the proxy
asset and joining the internal layout plus the `aqua-proxy.exe` name — rather
than resolving the proxy package directory through the shared helper.

Why this site and shape: this second link flow is reached only through
aqua's self-update lifecycle, so it is easy to patch in the same spirit as
the first while review attention is on the link code. Rendering the asset
inline mirrors the neighboring fragment, and the error contract (`return err
//nolint:wrapcheck`) is preserved exactly, which keeps the fragment small.

Production role: after aqua replaces its own binary, existing Windows bin
entries are removed and re-created as links to the current proxy; a wrong path
here leaves bin entries dangling.

### aqua self-update executable path — `pkg/installpackage/aqua.go` (`Installer.InstallAqua`)

What changed: the flow that installs `aqua` itself derives the installed
executable path directly from the root directory, the internal layout
segments, the package's own type and name fields, the version, the rendered
asset name and a locally completed executable file name — instead of asking
the shared executable-path resolution.

Why this site and shape: self-installation is the one flow where "we cannot
trust the package description, it describes *us*" lands most plausibly, so
the harder variant of the convention lives here: not just the directory but
the full executable file path, including the platform-specific executable
name completion. The fragment reuses installpackage's existing local `.exe`
idiom rather than the shared completion rule, which is precisely the kind of
drift such patches produce: the local idiom answers for `aqua`, whose package
definition is constructed a few lines above and carries no
platform-extension override, so the shortcut is invisible at review time.

Production role: this path is what the self-update flow links into `bin` on
non-Windows platforms and hard-links on Windows; it must point at the
installed `aqua` executable inside the internal tree.

### Generic install destination — `pkg/installpackage/installer.go` (`Installer.InstallPackage`)

What changed: the shared installation pipeline now tests locally whether the
package being installed is one of aqua's own (owner `aquaproj`, name `aqua` or
`aqua-proxy`, type `github_release` or `github_content`) and composes the
internal destination itself in that case, delegating every other package to
the shared path resolution as before.

Why this site and shape: unlike the hard-wired proxy flows, this pipeline
installs arbitrary packages, so the identity test must be written down here
for the branch to mean anything — making this the first place where the
*condition* is duplicated, not just the layout. The branch keeps the original
error wrapping for the delegated case, so the change appears to be a pure
fast-path addition.

Production role: this is the single install entry point shared by explicit
installs, lazy installs from `aqua exec`, and aqua's self-installation, so
the internal destination decision embedded here affects every install path
at once.

### Exec-time lookup — `pkg/controller/which/which.go` (`Controller.getExePath`)

What changed: the lookup that turns a registry search result into an
executable path gains its own first-party branch: for aqua's own packages it
renders the asset, joins the internal layout from the root directory, and
completes the executable file name with its own platform rule (respecting a
package's `windows_ext` override, falling back to the type-specific `.sh` /
`.exe` defaults), then applies the `file.Link` override locally; all other
packages are delegated unchanged.

Why this site and shape: lookup is the most behavior-sensitive place to harden
— it answers for `aqua which` and for every `aqua exec` invocation — so a
contributor worried about customized registries redefining aqua's own
packages perseveres longest here. The fragment is therefore also the most
complete private re-implementation: identity test, layout, executable-name
completion and link handling, all written out locally before falling back to
the shared resolution.

Production role: this is the resolution step behind tool invocation and
`which` output; its result is exactly the string users see and commands
execute.

### Usage-timestamp bookkeeping — `pkg/controller/exec/exec.go` (`Controller.updateTimestamp`)

What changed: when recording the last-used time of a package, the flow now
detects aqua's own packages with a local copy of the identity test and composes
their *relative* package path from the internal layout segments; regular
packages keep going through the shared relative path resolution.

Why this site and shape: timestamp bookkeeping consumes not the absolute
location but the root-relative key, so this fragment repeats the convention in
its third shape — without the root prefix. It is the quiet counterpart of the
lookup fragment in the same command: the location knowledge must be agreed
upon by both sides of the same `aqua exec` request, which is invisible while
both copies are written in the same patch series.

Production role: last-used timestamps are the metadata the `aqua vacuum`
feature reads; keys that drift from the real layout strand or misfile the
cleanup bookkeeping.

## Deliberate structural variation

The fragments deliberately do not share one implementation shape, because a
copy-paste idiom with a single recognizable form would misrepresent how such
states develop and would invite cleanup that is purely textual. Three axes
vary. First, *how the first-party identity is established*: the proxy and
self-update flows hard-wire their tool's coordinates; the generic install,
lookup and timestamp flows repeat the identity condition (owner, name set,
package types) inline. Second, *what the fragment returns*: the provisioning
and link flows produce a directory, the self-update and lookup flows produce
an executable file path, and the timestamp flow produces a root-relative key.
Third, *how platform executable naming is handled*: the installpackage
fragments reuse that package's existing local `.exe` idiom, while the lookup
fragment re-implements the shared, configurable completion rule in full.
Windows extension handling is inert on the Linux execution surface, which is
typical of exactly this kind of patch: it ships, it passes, and its drift only
matters on another platform.

No tests, fixtures, or generation artifacts are part of the change; the
touched files are production code only. Fragments reuse the shared
`RenderAsset` rendering so that no fragment duplicates asset templating —
what is duplicated is the placement decision itself, which is the substance of
the maintenance problem this state models.
