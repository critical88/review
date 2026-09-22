# Injection design record — previous-generation mechanism residue in diun

## Maintenance motivation

The diun 4.32 and 4.33 release waves replaced several platform mechanisms in
quick succession: the notification mailer moved from gomail to the go-mail
driver (#1732, #1733, #1734), the shutdown lifecycle was simplified around
context propagation (#1704), database manifest entries were reworked for
consistent handling (#1715), config defaults were modernized and obsolete util
helpers removed (#1705), and generated artifact tags and non-image artifacts
began to be skipped during registry checks (#1745, #1746).

Work of that kind rewires the top of a mechanism: the entry point, the
registration call, the loop that starts the work. The classic hazard of doing
only that rewiring is that the stratum beneath it survives. The helpers, data
shapes and one-off routines of the previous generation stay compiled in, keep
referencing each other, still read like the infrastructure of the product, and
no longer sit on any path the program actually takes. Readers have no easy way
to tell that state apart from a feature: the references inside such a family
are real, the symbols are often exported, the idiom matches the neighborhood,
and per-file reasoning finds a decorated call chain instead of a boundary.

This change models a repository in exactly that intermediate state. The
replacement mechanisms are complete and in daily use; a handful of families
from the previous generation of each mechanism remain in the tree.

## Modeled evolution

The added material is residue of four mechanisms that the recorded waves would
naturally have retired.

1. **Fixed-format end-of-run digest mail.** Before notification titles and
   bodies became configurable templates (`model.NotifDefaultTemplateTitle` /
   `model.NotifDefaultTemplateBody`, per-notifier `TemplateTitle` /
   `TemplateBody` options), a completed watch run was summarized as a digest
   mail: a fixed title template and a fixed multiline body summarizing the run.
   When templated per-entry notifications landed, the digest was dropped rather
   than ported. The run loop kept only entry aggregation for its status counters
   (`model.NotifEntries`). The residue spans three packages: the run-summary
   type and its fixed rendering in `internal/model`, the title template, its
   parsing helper and execution context in `internal/msg`, and the renderer and
   subject composer in `internal/notif/mail` next to the go-mail based send
   path that replaced them.
2. **One-off manifest re-keying pass.** Before manifest entry keys carried the
   tag suffix — today `PutManifest` keys entries by `image.String()`
   (`name:tag`) and `First` seeks by the `name:` prefix — a one-shot routine
   squashed bare-name keys into tag-qualified ones, defaulting a missing tag to
   `latest` and a missing media type to the Docker v2 manifest media type.
   Once the versioned migration chain (`dbVersion`, `Migrate`, `migration2`)
   became the only sanctioned path for storage schema adjustments, the one-off
   pass had no operator left to call it.
3. **Artifact-suffix tag filtering.** Before registry checks keyed on
   digest-qualified artifact notation, the mechanism of record for skipping
   generated artifacts was a tag-list filter that dropped `-att`, `-sbom` and
   `-sig` suffixed tags. The family sits in the public registry client next to
   the tag listing API it once fed.
4. **Signal-draining shutdown path.** Before the lifecycle simplification,
   termination handling lived in a loop that drained a termination signal
   channel, logged a human-readable reason, stopped the scheduler and failed
   the healthchecks ping. The run `Start` path now returns on context
   cancellation and server errors; the loop is what remained of the old
   arrangement.

## Overall design

The residue is organized as internally cohesive, cross-referenced families.
Each family is wired like the mechanism it came from — one family is renderer,
template helper, template constant, execution context, data type and rendering
method together; another is exported entry routine, legacy layout type and the
type's methods; a third is a filter consumer and its leaf predicate; a fourth is
a drain loop and its reason mapping. Within a family every declaration is
genuinely referenced, so name-by-name inspection finds call chains, not
verifiable callers; the exported members look like the public surface of their
packages; the unexported members take real runtime objects of the application
(scheduler client, signal channel, db client), so no reader can dismiss them as
stub shapes.

Structurally the additions vary deliberately: five files receive appended
declarations at the end of the file; the registry family is inserted mid-file
between two live functions of the public API; four files grow import-block
entries; one file grows a small const block grouped with the template machinery
it belongs to. No existing declaration body is modified and
no test file is touched; the diff is additions-only (200 insertions, no
deletions).

## Per-cluster explanations

### `internal/app/app.go` — `watchTermSignals`, `termSignalReason`

The app package owns the run lifecycle (`New`, `Start`, `Run`) and already
deals in scheduler state, healthchecks pings and, in the old arrangement, the
termination channel. The drained-loop shape is what the shutdown path of the
pre-simplification era looked like: for each received signal, map it to a
message, stop the cron scheduler, fail the healthchecks run and return. The
reason mapping lives in a separate pure helper, mirroring how the historic loop
split draining from message formatting. The pair takes a `*Diun` and a
`chan os.Signal`, so it reads as part of the application wiring rather than as
dead scaffolding; the production role it models is the retired termination
path of the scheduled watch.

### `internal/model/notif.go` — `RunDigest`, `(RunDigest).RenderText`

The model package owns notification contracts (`Notif`, `NotifEntries`,
`NotifEntry`), so the run-level summary type of the digest era belongs here. The
fields (`StartTime`, `EndTime`, `Entries`, `Hostname`) are the data a run-end
digest plausibly summarized; the rendering is a fixed multiline format built by
writing straight into a `strings.Builder`, the pre-template rendering style,
rather than a template. The production role is the data shape and fixed-format
output of the retired digest content.

### `internal/msg/template.go` — `digestMailTemplateTitle`, `DigestMailTemplate`, `DigestMailContext`

The msg package owns notification templates (`templateFuncs`, the per-entry
message construction in `client.go`), so the digest title template, its parsing
helper and the context it executes against fit its local habits: the parser
reuses `templateFuncs`, and the const is grouped with the machinery that
consumes it exactly the way the per-entry template titles are grouped in the
model package. The context carries the meta plus the run digest, the two
things a digest mail title was rendered against. The production role is title
templating of the retired digest mail.

### `internal/notif/mail/client.go` — `renderDigestMail`, `digestMailSubject`

The mail notifier is where a digest mail would have been delivered, so the
retired renderer lives next to the send path that replaced it, appended after
`Send` and `mailClient`. The renderer composes subject and body from the template
pipeline and the fixed summary rendering; the subject helper applies the
hostname and product-name prefixes the subject lines of the era carried. Kept
unexported because nothing outside the package ever produced a
`model.RunDigest`. The production role is message assembly of the retired
digest delivery.

### `internal/db/migrate.go` — `manifestEntryV1`, `(*manifestEntryV1).sanitize`, `(manifestEntryV1).key`, `SquashManifestEntries`

The migrate file is the storage layer's migration surface, which is where a
retired re-keying pass reads most naturally. The exported routine follows the
transaction idiom of the neighboring `migration2` (`Begin(true)`, deferred
`Rollback` tolerating `ErrTxClosed`, bucket cursor walk, `Commit`), so its shape
matches the sanctioned mechanism that superseded it; the flattened layout type
mirrors `migration2`'s `oldManifest` fields; `sanitize` supplies the defaults a
pre-tag database would have lacked (missing tag, missing media type); `key`
produces the `name:tag` key that the current entry helpers expect. The
production role is a one-shot operator pass to bring very old databases onto
tag-qualified keys, from before the versioned chain made it obsolete.

### `pkg/registry/tags.go` — `isArtifactSuffixTag`, `filterArtifactSuffixTags`

The public registry client owns tag listing, so a tag-filter family placed
between `Tags` and `SortTags` reads as a helper extension of that API. The
shape is a leaf predicate over the artifact suffixes of the era plus a consumer
that returns the remaining tags and the number dropped, which is what a caller
logging skipped artifacts would have wanted. No migration shim or digest
notation exists to feed it now. The production role is the retired generated
artifact skip for tag lists.
