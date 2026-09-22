# Injection design record — gitleaks staged-rollout residue cleanup case

## Maintenance motivation

Gitleaks accumulated its 8.x behavior in waves: multi-pass decoding of encoded
segments, archive-aware file traversal, multi-part (required) rules, and the
long-running 9.x preparation (config format slimming, the `protect` command
being reduced to an alias). Work of that kind normally ships behind small
staging levers — a package-level toggle pinned in source, a compatibility
level compared against a threshold, or a draft helper parked beside the code it
was going to serve — so that the feature can be finished or dropped without
re-churning the surrounding pipeline.

This case models the aftermath of that style of development: the preparation
era produced several pieces of machinery whose rollout was postponed or whose
surrounding feature shipped in a smaller shape, and the leftover levers and
drafts stayed in the tree. Nothing dramatic is wrong — the scanner behaves
exactly as released — but the tree carries code that can no longer execute
under any input, which is precisely the kind of residue that misleads readers
preparing the next release: every one of these regions looks like a live
feature path until someone traces the constants and callers.

## Normal evolution being modeled

The injection reproduces a recognizable end state of normal feature staging:

1. A pass is added to the multi-pass decode loop behind a toggle, the toggle
   is pinned to a constant while the segment cache settles, work moves on, and
   the toggle never flips.
2. A compatibility behavior for older configs is drafted behind an integer
   "compat level" held below its gate, with the helper implementing the
   compat behavior sitting in the layer below.
3. A preview facility (summarize what a deep archive scan would cover) is
   drafted as a small type next to the feature that did ship; only the
   feature ships.
4. Report shaping helpers are drafted next to the writers they would have
   served; the writers keep their existing path.
5. Remote-configuration extend support is anticipated beside the existing
   local-extend machinery, deferred with a TODO, and the validation draft
   never gets a caller.
6. A deprecation window keeps a legacy exit-code contract handled by a small
   mapping layer; the mapping never gets wired into the command.

Each of these shapes is the residual state of an ordinary, defensible
engineering decision; the injection authored the residue directly rather than
replaying the history that would have produced it.

## Overall design

All injected code is additive and behavior-preserving: existing statements,
declarations, and their execution order are untouched, so the scanner's
observable output is identical. Two injection forms are used, matching how
staged features actually park in a Go codebase:

- *Gated branches in live functions.* A package-level constant gate and an
  `if` guarded on it (boolean toggle in the detection loop) or on a comparison
  between package constants (integer compat level in the codec decode path).
  The guard conditions evaluate to `false` at compile time, so the branch body
  can never run, while the declarations referenced in the body appear in the
  type-checked source and therefore read as connected code.
- *Orphaned draft declarations.* Unexported helpers, a small type with its
  constructor and methods, and constants that exist to be referenced by those
  drafts. Some are unreferenced outright; others are referenced only from
  other dead code (chained drafts) or only from inside the gated branches,
  which is the shape real staged code takes when the last live caller was a
  feature that never landed.

The residue is spread over the components that actually own those lifecycle
phases: the detection engine and its detector helpers, the codec layer below
it, the file/archive source front end, two report writers, configuration
loading, and the command layer. Comments on the injected declarations
narrate the staging story (why the code is parked), in the voice the codebase
already uses for its TODO/staging notes.

## Per-location rationale

### Multi-pass decode re-probe (detection loop + detector helpers)

The decode loop is where a staged pass would really be inserted: after a pass
decodes, a re-probe would re-run the keyword prefilter on the decoded text so
keywords that only surface after decoding are not lost for the next pass. The
gate is a boolean constant in the detector's existing constant block next to
`SlowWarningThreshold`, pinned off with a staging note; the branch sits
exactly between the decode call and the loop-exit condition, where such a pass
would run, and feeds results back through the loop's own result variables. The
re-probe implementation was placed in the detector helper layer as a method on
`Detector` (it needs `Detector.Config.Keywords`), and it delegates keyword
extraction to a plain helper — the natural method-to-helper chain this
codebase uses. The pair is reachable only through the gated branch, so it
reads as live pipeline code until the gate is traced. This location gives the
detection pipeline a two-level dependency chain behind a compile-time
toggle.

### Codec compatibility merge (decoder gate + segment merge helper)

The second gated form avoids repeating the boolean-toggle shape: a paired
integer gate (`decodeCompatLevel` held below `decodeMergeGate`) in the codec
layer, whose merge call folds adjacent decoded segments so later passes treat
pairs the old boundaries split apart as one span - the kind of compat shim a
segment-boundary change would really stage for older configs. The comparison
form is placed at the end of `Decoder.Decode` where post-pass segment cleanup
belongs; the merge helper lives in the segment file beside the segment
construction helpers it composes (`merge`, predecessor concatenation,
encoding flags), using only existing segment machinery. This location provides
the integer-comparison gate form and a free helper referenced solely by a
gated branch, in the layer below the detector.

### Archive-listing preview cluster (file source front end)

The archive-depth feature in the file source is realistic abandoned-preview
territory: before opting into deep scanning, a caller would list what the
scan would cover. The cluster is the natural shape of such a draft — a small
unexported struct (`path`, member entries, remaining depth), its constructor,
an iterator method that classifies nested archives using the existing
`isArchive` helper, and a driver method that renders the listing line by
line. The methods reference each other and the type, so the cluster is
mutually coupled and reads as a self-contained subsystem parked next to the
shipped feature. It sits between the file-source traversal (`scanTargets`)
and the fragment walker that consumes it, i.e., exactly where a preview
consumer would have been wired in. This location contributes the type-cluster
and mutually-referencing-chain forms inside a real component boundary.

### Report shaping drafts (JSON and JUnit writers)

Two writers provide complementary orphan shapes in a single component family.
In the JSON writer, a fingerprint-based dedupe helper: drafted so repeated
scans of the same range would not emit duplicate findings, never wired into
`JsonReporter.Write`. In the JUnit writer, a helper that renders a multi-part
(required) rule's auxiliary findings as their own test cases, drafted when
required rules landed; it reads from the finding's private required-findings
slice and builds `TestCase` values through the writer's own field mapping,
which makes it look like part of the report path until its call site is
searched for. These two locations contribute plain package-level orphans next
to live production writers that the codebase already exercises through
fixtures.

### Remote-extend validation draft (config loading)

Config extend handling contains the existing `extendURL` TODO stub - the
codebase's own marker for anticipated remote-extends. The draft placed beside
it follows the anticipated contract: a depth bound constant plus a validator
that a remote extend path would be checked against before loading. It uses
the config package's own error and string idioms, compiles against the live
`Config` surface, and by construction has no caller because remote loading
was never wired up. This location gains its plausibility from the adjacent
stub: the region was already marked as future work, so a validator parked
there does not look alien, and the two-branch check keeps the validator from
being a trivial one-liner.

### Legacy exit-code compat layer (command layer)

The `protect` deprecation story fits the command layer: while `protect` was
being reduced toward an alias of the git pre-commit scanning path, wrapper
scripts depended on the old command's exit codes. The draft that follows is a
pair of constants plus a resolver function that maps a deprecated invocation's
code onto the documented space. It was placed after `runProtect` where a
follow-up mapping call would sit; the resolver references the constants and
nothing references the resolver, so the whole contract is present but
unwired. This location contributes the constants-referenced-only-from-dead-code
form (both constants and resolver are flagged as a coupled set) and puts the
residue in a fourth component — the command surface — with behavior-adjacent
code (exit codes) to make over-eager edits risky.

## Integrity constraints maintained while authoring

Every declaration added is unexported, so no public API changes. The gated
branches execute `false` without side effects, assign only variables already
being reassigned in the loop, and reference only declarations that exist for
them. No test file, fixture, or configuration file was touched, and no live
statement was deleted, reordered, or wrapped. The full repository module
still builds, formats, and passes its complete test suite in the injected
state, exactly as it did before the injection.
