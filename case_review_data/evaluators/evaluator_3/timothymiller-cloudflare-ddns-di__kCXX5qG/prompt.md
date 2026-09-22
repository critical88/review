# Refactor the update pipeline's service layers: restore method boundaries

## What we observed

The repository's update pipeline went through a performance-hardening pass
that we now need to unwind at the design level. During that pass, several
service-layer methods stopped delegating to their helper methods and instead
took over their implementations: bodies of lower-level methods were copied
into the callers that used to invoke them, lightly adapted with hoisted local
bindings. The helper definitions still exist, but production traffic now
flows through the embedded copies, so the same request and filtering logic
exists in several places at once.

The areas the pass touched are the network-facing service layers of the
update pipeline:

- the Cloudflare API integration (zone-record listing and filtering, record
  create/update/delete operations, WAF list maintenance);
- the legacy-mode Cloudflare update path (per-detected-IP record commit and
  the batch update over it);
- provider-based public IP detection (dispatch over providers, including the
  local-socket detection routine).

## The problem this creates

The abstraction steps these helper methods used to own — building and sending
one HTTP request with its error reporting, listing records for a zone,
filtering them to one DNS name, committing one record entry, probing the
local address — are now reproduced inline inside the orchestration methods
that call them. That duplication is concrete and layered: some methods embed
copies of helpers that themselves embed copies of lower helpers, and the
copies are interleaved with the surrounding original logic (comparison
branches, dry-run guards, message accumulation) so that the boundaries
between the levels are no longer obvious. Beyond the review and debugging
cost, any future change to request construction, endpoint shapes, or error
reporting has to be found and re-applied at each embedded site; the helpers
that still claim ownership of those responsibilities no longer own them in
practice.

## What we want

Restore the intended layering across the affected service layers: each
distinct operation — request plumbing, zone-record listing, name-filtered
listing, individual record create/update/delete, each WAF maintenance step,
single-entry record commit, per-IP commit, local-socket detection — should
have exactly one owning implementation in the production code, and the
methods that orchestrate those operations should express the steps through
those implementations (including at loop sites and match arms where copies
were embedded).

This is a repository-level refactoring concern, not a single-method edit:
investigate all three service layers listed above and address every instance
of the same underlying pattern, wherever you find it. Where you consolidate a
duplicated operation, remove whatever scaffolding (hoisted adapter bindings,
temporary request locals) exists only to feed the embedded copies.

## Compatibility boundary

This must be a pure refactoring:

- observable behavior is unchanged, and the repository's full test suite must
  keep passing with no new failures;
- the command-line surface, configuration format and parsing (modern and
  legacy modes), environment variables, stdout/stderr message wording, and
  exit codes are untouched;
- HTTP endpoint shapes stay semantically identical for every operation
  (paths, query parameters, HTTP verbs, request bodies);
- dry-run behavior, no-op suppression reporting, and WAF list cleanup
  behavior are preserved exactly;
- IP detection semantics per provider and address family are preserved.

You may restructure internals freely — rename helpers, adjust private
signatures, add parameters, or relocate private code between the affected
modules — provided these constraints hold.
