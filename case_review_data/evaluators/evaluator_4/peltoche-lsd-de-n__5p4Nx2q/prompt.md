# The listing export we never shipped is still in the tree

## What I ran into

While putting `lsd` on a diet I went looking for easy wins and instead found an old experiment
that was never cleaned up. A while back a machine-readable export of a listing was prototyped —
plain rows to be parsed by tools instead of the rendered grid people read — merged behind
per-channel rollout switches, and then shelved when the rollout never went ahead. The platform
moved on; the code never left, and it has been riding along in every build since.

Nothing in the CLI or the config file activates it, and nothing outside the feature itself calls
into it. I expected deleting it to be a five-minute job, but the annoying part is that the
ordinary unused-item diagnostics stay completely silent: the members still reference each other,
and real functions along the pipeline still reference them — from branches that, if you look at
their conditions, are governed by channel switches that are permanently off. So the whole thing
looks wired-up while nothing in it can ever execute.

It also doesn't sit in one place, so no single deletion finishes it. The remains run along the
listing pipeline itself: the runner that fetches, sorts and displays, and the helpers it drives —
the grid rendering, the icon resolution chain, the ordering code, the entry metadata. In each of
those places the export-only members sit next to live members doing the same kind of work, so
it isn't a matter of deleting a block; the live pipeline members have to be told apart from the
export ones.

## What I want

The tree to read as if the experiment had never been merged. Everything that only exists to
serve the machine-readable format should go: the activation branches and the channel switches
that never enabled them, and the members behind them, down to helpers and lookup variants whose
only callers are other export members. Completeness matters more than minimal diff size here —
if I search the affected modules for stragglers afterwards, what should be left is the pipeline
and nothing it doesn't need.

## Don't break the product

The export is inert, so removing it must be invisible to users. Rendered grid, tree and one-line
output identical for any flags and any directory; icon resolution identical; the orderings users
see identical; CLI flags, defaults and config keys unchanged. If a hunk of the change touches
anything that renders, resolves icons, or orders live output, it should be because it deletes
export-only members — not because live behavior needed adjusting.
