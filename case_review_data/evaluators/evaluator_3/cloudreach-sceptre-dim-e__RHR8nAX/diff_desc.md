# Injection design record

This record documents why the two production sites in `smell.diff` were chosen
and how their inlining was shaped. It is auditable design rationale, not an
assessment of what the diff is or is not.

## Maintenance motivation

Two areas of sceptre absorbed inline logic for plausible, non-malicious reasons
during ordinary maintenance.

### Configuration ingestion — `sceptre/config/reader.py`

`ConfigReader._read` is the lowest-level entry point for loading a StackGroup
file: every configuration read funnels through it, and it recurses up the
directory chain rendering and merging layers. During work focused on cold-start
config reads, a maintainer wanted to remove the per-layer call overhead and
make the inheritance walk legible in one place. The recursive layer reader and
the per-key merge helper were folded directly into `_read`, the recursion was
replaced by an explicit directory-chain walk so the whole traversal reads
top-to-bottom, and a small debug call was added at each layer for tracing reads
in the field. The two absorbed helpers stopped being called and were removed
to keep the module free of dead code.

### Stack construction — `sceptre/stack.py`

`Stack.__init__` validates inputs and reconciles the deprecated alias fields
onto their preferred attributes. A maintainer tightening the constructor wanted
every value-coercion and alias-resolution step visible in one place so the
validation and the deprecated-alias redirection could be audited without
chasing four small helpers. The boolean guard, the deprecated-alias resolver,
and the parameter caster were inlined into the constructor, and the helpers
were removed.

## Normal development evolution being modeled

The change models a "fold the helpers in so this entry point is locally
legible, then delete the now-unused helpers" pattern — a realistic
refactoring-turn where a developer optimizes for local readability and call
cost and removes what they believe is dead code, while the surrounding long
method absorbs several distinct responsibilities that used to be separated.

## Overall design

Two production owners, one per component, each absorbing the bodies of helpers
that existed only to be called from that owner.

- `ConfigReader._read` absorbs the recursive layer traversal and the per-key
  merge-strategy resolution. The traversal is rewritten from recursion to an
  explicit chain walk (root to leaf), and the merge body is inlined twice —
  once per layer and once against the base config — because it is needed at
  both points. The collaborators that stand for legitimate separate concerns
  (template rendering, version compatibility) are retained as delegations
  called from inside the inlined body.

- `Stack.__init__` absorbs the boolean guard (inlined three times for
  `disable_rollback`, `ignore`, `obsolete`), the deprecated-alias resolver
  (inlined four times for the role session duration, the role, the service
  role, and the template handler config), and the parameter caster (nested as
  two small local transforms). The deprecated-alias fallbacks keep using
  `setattr` so the existing deprecated property setters still redirect each
  value onto its preferred attribute (for example `iam_role` onto
  `sceptre_role` and `template_path` onto a `template_handler_config` of
  `{"type": "file", "path": value}`), which is the redirection the rest of the
  system relies on. The `template` config-key resolver is inlined with its
  required branch folded into the final `else`, reflecting that the template
  configuration is mandatory for that call.

## Per-location rationale

### `sceptre/config/reader.py` — `ConfigReader._read`

This site was chosen because the whole `_read` to layer-render-and-merge chain
reaches outward through collaborators that are kept as delegations, while the
two helpers absorbed into `_read` had no callers besides `_read`, so the
behavior reachable from outside is unchanged by the fold. The
recursion-to-iteration change is a deliberate idiom shift so the inlined form
does not read like a trivial copy of the recursive helper, and the merge body
is copied at both points where it was previously invoked because each point
needs the same computation. Template rendering and version compatibility stay
as calls because they are genuine collaborations rather than absorbed
local responsibilities.

The product role: `_read` is the single point through which a StackGroup
configuration is read and inherited, so centering the whole inheritance
computation there is a credible localizing move during a hot-path pass.

### `sceptre/stack.py` — `Stack.__init__`

This site was chosen because the three absorbed helpers had no callers besides
the constructor, so folding them in is behavior-preserving, while the
deprecated-alias redirection depends on `setattr` reaching the deprecated
property setters — so the inline copies reuse the identical `setattr` calls the
helper used, preserving the redirection onto the preferred attributes. The four
alias sites differ in which names and which `required` behavior they carry, so
each inline copy is specialized to its arguments rather than being a uniform
expansion.

The product role: `__init__` is the constructor that validates and normalizes
every user-supplied Stack field and parameter before a `Stack` enters the rest
of the system, so folding the whole normalization pipeline into it is a
credible auditing-the-constructor move.

## Behavioral boundaries preserved

The inlined `_read` walks the same chain (including the config-root layer),
renders each layer through the retained template renderer, applies the same
per-key merge strategies and overrides, runs the same required-version check,
and returns the same merged configuration. The inlined constructor raises the
same validation errors for non-boolean flags, mutually-set deprecated pairs,
the missing required template configuration, non-dict parameters, and invalid
parameter expression shapes, and casts booleans, numbers, nested lists, and
Resolver instances exactly as the prior caster did.

The fold touches no public interface: the inlined owners keep the same
signatures, call sites, and instance attributes as the originals, so every
caller of `ConfigReader` and `Stack` is unaffected.
