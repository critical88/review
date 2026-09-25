# Injection design record: the link-check pipeline as owner of everything it dispatches

## Maintenance motivation

lychee's central link-check pipeline lives in `lychee-lib/src/client.rs`. A
`Client` receives a `Request`, prepares the URI, applies the configured rules,
dispatches to the right kind of check, and reports the resulting `Status`.
Around it sit small, focused collaborators: the exclusion policy configuration
(`filter`), the remap rule table (`remap`), and a family of per-protocol
checkers (`checker::{file, website, mail, wikilink}`), each owning exactly one
job.

The maintenance problem modeled here is the slow drift of such a pipeline
type from *dispatcher* to *implementor*. Pipelines are attractive places to
"just handle it here": every change request already touches the pipeline, the
collaborators feel far away, and each individual absorption looks like it
saves a level of indirection. What the pipeline gains instead is a second copy
of policy it does not own, private knowledge of other components' internals,
and attached machinery whose purpose no longer matches its address.

## Evolution modeled

The change mimics a believable sequence of small, locally-reasonable commits
in which the pipeline type takes over three responsibilities that real former
owners can still prove existed at this commit:

1. **Exclusion policy.** Instead of asking the filter configuration whether a
   URI is excluded, the pipeline evaluates the full policy chain itself — the
   scheme and host rules, the IP classifications, the mail rule, the
   built-in reserved/unsupported domains, the false positive patterns, and
   the include/exclude regular-expression precedence — by reading the
   configuration owner's fields directly.
2. **Remap resolution.** Instead of letting the remap rule table evaluate
   itself, the pipeline opens the table, walks its rule list, applies the
   first match itself, rewrites the URI, and rebuilds the result, preserving
   the first-match-wins order and the invalid-remap-target failure of the
   original wording. To make this possible the rule table's pattern list,
   previously an implementation detail, is readable by the whole crate.
3. **Mail verification.** The dedicated mail checker component — the only
   owner of the mailify verification engine — is deleted, its module
   registration is removed, and the verification engine becomes a field of
   the pipeline together with the verification procedure itself, with two
   feature-split variants of the public dispatch method so mail URIs stay
   excluded when the `email-check` feature is off.

Each step has the shape real code history shows: an inline of an
indirection, "temporarily" duplicated policy that is never factored back, and
a component declared unnecessary once its one caller stopped calling it.

## Design of the change surface

The three absorbed responsibilities touch four production files:

| File | Change |
| --- | --- |
| `lychee-lib/src/client.rs` | the pipeline struct gains the three responsibility bodies, their state, and supporting probe helpers |
| `lychee-lib/src/checker/mail.rs` | deleted: the previous owner of mail verification |
| `lychee-lib/src/checker/mod.rs` | module registration of the deleted owner removed |
| `lychee-lib/src/remap.rs` | the rule table's pattern list becomes crate-readable so the pipeline can walk it |

## Per-location rationale

### `client.rs` — exclusion policy island

**What.** `Client::is_excluded` stops delegating to the filter configuration
and evaluates the complete policy chain itself. The chain is decomposed into
one small question per policy category (`is_scheme_excluded`,
`is_host_excluded`, `is_ip_excluded`, `is_mail_excluded`,
`is_builtin_excluded`, `is_includes_empty`, `is_excludes_empty`,
`is_includes_match`, `is_excludes_match`), each reading the filter
configuration's fields directly. The static false-positive and domain
predicates are imported from the filter module and consulted from the
pipeline.

**Why this site and shape.** This is the largest responsibility of the three
and the one whose re-homing is most defensible in a code review, which is
precisely why it survives in real projects: the reviewer sees tests pass, the
policy is preserved down to the precedence order, and nobody owns the fact
that the *pipeline* now has to be edited whenever an exclusion category
changes. Splitting each policy category into its own helper is the natural
shape of the inlined chain — it mirrors the filter module's own structure,
which makes the duplication easy to miss and leaves the pipeline reading
another component's configuration field by field. The category helpers are
private to `Client`, so nothing outside notices the move.

**Production role.** Exclusion decisions on every checked link, before
dispatch.

### `client.rs` — remap resolution island

**What.** `Client::remap` no longer calls the remap table's own evaluation
method; it gains a `has_remap_rules` probe, used by `check` to fast-path the
no-rules case, and a `resolve_remap_target` body that walks the rule list,
matches, replaces, parses the result, and constructs the remap result object
itself. Error text and matching order match the previous owner exactly.

**Why this site and shape.** The dispatch site in `check` already needed a
"skip if nothing configured" answer, and resolving inline avoids what a
developer in a hurry calls "a needless indirection" — the table object
cooperating in rewriting a URI it does not consume. Keeping the probe helper
gives the pipeline a permanently useful-looking reason to know the rule
table's emptiness, and keeping the evaluation next to `check` keeps the
rewrite visibly coordinated with the URI the pipeline mutates. The
first-match-wins loop and the rebuilt-from-scratch result construction
translate the previous owner method one-to-one, so the seam between "pipeline"
and "policy" quietly moves into the wrong place without behavioral evidence
of the move.

**Production role.** Rewriting configured links to the URLs actually checked,
at the head of every request.

### `client.rs` — mail verification island

**What.** The pipeline's struct gains an `email_verifier` field holding the
mailify engine (present only under the `email-check` feature). The public
`check_mail` splits into two feature-gated variants with the same signature;
under the feature, it goes through `has_mail_verifier` and `mail_verifier`
probes and a new private `perform_email_check` that owns the address
extraction, the engine call, and the verification-result-to-`Status` mapping.
Without the feature, it returns excluded, with an allowance for the
signature-preserving unused-async body, the same convention the deleted owner
used.

**Why this site and shape.** Absorbing a *component object* rather than just
code is what makes this island materially different from the other two: the
engine, its construction, and its feature split all move address. The
`Option` wrapper plus probes reads as defensive API design ("verify before
verifying"), which is exactly the kind of scaffolding that grows around an
absorbed responsibility. The constructor entry in `ClientBuilder::client`
builds the engine alongside the other pipeline fields, so construction
continues to work in both feature states and the deleted module leaves no
compilation trace. The doc comments on the field tell the story the code now
embodies: mail verification "started out as its own checker object" but the
client "took over the verifier itself" — the honest remnant of the old owner
being deleted rather than kept.

**Production role.** Deciding whether a `mailto` address is reachable, and
with which mapped status, under the mail feature.

### `checker/mail.rs` deletion and `checker/mod.rs` rewiring

**What.** The mail checker module — struct, engine construction, feature
variants, and verification procedure — is deleted, and its module
registration disappears from the checker family.

**Why this site and shape.** Deleting the former owner rather than leaving it
as dead code is what a developer cleaning up "the extra file with the
duplicated function" actually does once its last external caller is gone: it
would otherwise fail as unreferenced module content. The registration removal
in the module list is the minimal consistent edit. This is the part of the
change surface that makes the absorption irreversible without real
reconstruction work: to undo it, the module has to be brought back and rewired
into the checker family and the construction path, in both feature states.

**Production role.** None remaining — that is the point: the checker family
is no longer complete, and mail checking is reachable only through the
pipeline.

### `remap.rs` — exposed rule list

**What.** The rule table's pattern list changes from a private field to a
crate-readable field with a short evaluation-order doc comment.

**Why this site and shape.** The smallest edit that makes the absorption
possible, and the classic cost signature of absorbing a responsibility: a
visibility widening that exists solely so another component can operate the
owner's internals. Keeping the field marked crate-visible (rather than fully
public) keeps the edit reviewing as collaborative.

**Production role.** Storage and evaluation order of remap rules; the table's
own methods are unchanged and continue to serve its other callers.

## Behavioral preservation

The ported logic is a faithful, line-level translation of the previous
owners: the exclusion chain keeps its precedence order and every input class
it consults; the remap walk keeps first-match-wins order, in-place URI
rewrite, and the invalid-remap-target failure with the original wording; mail
verification keeps the address extraction, engine call, result mapping, and
the disabled-feature behavior. The feature split is carried over wholesale, so
compiling with and without the mail feature produces the same public
signature set and the same observable decision for mail URIs; no public
signature changes and no configuration meaning changes anywhere on the
surface.
