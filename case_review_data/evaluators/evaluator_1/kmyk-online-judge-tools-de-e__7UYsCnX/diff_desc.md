# Injection design record — retired pre-12 pipeline behind permanently disabled compatibility switches

## Realistic maintenance motivation

`online-judge-tools` released 12.0 as a coordinated break: the presentation of test results,
the modes used to compare outputs, the rules of the format strings, the names of some
subcommands, and the format of the download history all changed at once. Coordinated breaks of
this kind cannot land as one commit for the users, so the natural engineering pattern — used
by this project and by countless CLI tools before it — is a temporary compatibility layer: a
module of boolean switches (`use legacy rendering?`, `use old comparison modes?`, `use old
format rules?`, `accept old subcommand names?`, `import old history format?`) reading one
integer level constant, with the old implementations retained behind those switches until the
migration grace period ends.

The motivation modeled here is the *end* of that grace period: the old pipeline is retired,
the switch level is nailed to its floor, and the retired implementation is left sitting in the
tree "to be removed gradually". Nothing dramatic is wrong with the code — it type-checks, it
imports cleanly, nothing visibly breaks — and that is exactly why it survives for years:
every reader assumes somebody still needs it, every refactor routes around it, and every new
contributor pays for it in reading and in vying for space in the dispatch chains.

## Modeled development evolution

The diff represents the state a repository is commonly left in one milestone after a large
migration:

1. During the migration, the 11.x behaviors were kept working behind the switch module so
   that users could opt back in while adapting their scripts and special-judge setups.
2. At the 12.0 release the compatibility level stopped being configurable; the constant was
   pinned to the "fully retired" value, and the plan was to delete the old paths once the
   support tickets dried up.
3. This state is "some months later": the deletion never happened, but the code has settled in
   that shape — the compatibility module has acquired docstrings and level comments, the old
   arms have been reindented into their host dispatch chains during the post-release cleanup,
   and, in two places, the support for the retired mode has rotted one step further: the arms
   now select comparison modes that the current dispatch enum no longer even defines.

## Overall design

The retired pipeline is injected as one coherent leftover spanning the command package's real
responsibilities, wired exactly as a compat layer of this project would have been:

* a new switch module holding the pinned level constant, five zero-argument selectors
  with explanatory docstrings, and the level-selection comments;
* each switch guards one genuine production behavior it governs, placed at the point where
  that behavior is selected — not centralized dead files;
* the old implementations survive as dedicated helpers, each reachable only from the guarded
  region it serves, and one step of transitive deadness is included (a private rendering
  helper used only by the old renderer);
* two arms additionally select members retired from the current enumeration, representing the
  most-rotted form of the leftover: the arm stays long after the mode disappeared;
* all edits are production-side (no test, docs, or packaging edits).

The placement shapes are deliberately varied: the dead code appears as an `elif` in a
subcommand-dispatch chain, as function-head gates after docstrings, as discriminated
`if/elif` over an enum at two different call sites, as a nested `if` inside a control-flow
stub of a persistence method, as a method and a class on two different modules, and as a
module of its own. The leftover therefore does not concentrate into any single pattern a
reader could clip away in one motion: it is spread across positions of different syntactic
kinds.

## Per-location explanation

### `onlinejudge_command/compat.py` (new module)

The switch board: the level constant pinned to 0, the `# 0/1/2` legend comments, and five
selectors (`use_legacy_rendering`, `use_legacy_comparator`, `use_legacy_format`,
`use_legacy_command_names`, `use_legacy_history`), each documented as selecting part of the
pre-12 pipeline. Chosen as a module rather than inline booleans because that is how multi-
behavior v12-style migration gates are actually grouped in CLI projects; each function got a
docstring naming the behavior it selects, the usual shape for a public-facing compat surface.
The switches are referencable from every sibling module, so they give the five following
locations a *shared* selection mechanism rather than five local flags.

### `onlinejudge_command/main.py` — dispatch registry arm and its command body

At the end of the subcommand routing chain in `run_program`, an arm accepts the removed
`results`/`results-all` subcommand names when `use_legacy_command_names()` is set, backed by
`run_legacy_results_command`, a full module-level function reading the test subcommand's log
file and printing the old per-case summary. `import json` was added: this function is its only
user in the module. This location models how registry leftovers actually grow — the
subcommand's body is real code, parameterized by the argparse namespace, sitting one `elif`
away from live routing. It exercises dead code reachable from the *entry point* rather than
from a leaf, making the arm's deadness a property of the whole execution path. Semantically
the arm is guarded by a conjunction (a disabled switch *and* membership in the removed names),
the ordinary shape for a registry gate whose old keys must also match.

### `onlinejudge_command/subcommand/test.py` — comparison assembly and verdict display

Two placements in the local-test flow, both operating on the *policy level*:

* in `build_match_function`, a new first branch over `CompareMode.LEGACY_NORM_MATCH` guarded
  by the comparator switch, constructing the old comparator chain and holding the annotated
  `file_comparator` binding (the original first `elif` was adjusted accordingly). The arm
  sits ahead of every live mode. Unlike the dispatch else-stub arm, this guard relies on a
  mode value that has been *retired from the enum itself*: the arm is doubly dead — the
  switch is off and no `compare_mode` may overlap with its member. The left-side gate
  exercises a conjunction whose second leg is unanalyzable for a reader who does not check
  the enum definition.
* in `display_result`, inside the not-matched/printing stub, a leading branch replaces the
  output/expected printing with the old plain per-case diff, gated by the rendering switch,
  ahead of the four live display modes.

These two locations were chosen because the test subcommand is the project's flagship flow:
any leftover here directly burdens every current user's reading, yet nothing fails. They also
pair the same switch (`use_legacy_comparator`) and enum-member retirement with two call
sites — dispatch-side and builder-side — to exercise multiple occurrence shapes.

### `onlinejudge_command/pretty_printers.py` — old renderer and its token helper

The old plain-diff pipeline as `make_pretty_diff_legacy` (tokenize both sides through the
existing snipping-free tokenizer, render tokens without colors via `_render_tokens_legacy`,
then `difflib.unified_diff`), plus a gated head in the current `make_pretty_diff` that
delegates to it when the rendering switch is set. This branch positions dead code as an
*alternative rendering backend* — the documentable old sibling of the current colored
pipeline. `_render_tokens_legacy` is intentionally used only by `make_pretty_diff_legacy`,
giving the leftover a private, transitively dead layer — a helper whose deadness is only
visible through the deadness of its sole caller. The gated head was
selected as the cleanest integration point because `make_pretty_diff` is where every diff
consumer (summary-only, diff, and the old display-all styles) funnels.

### `onlinejudge_command/output_comparators.py` — retired comparator class and enum arm

A full class `LegacyNormalizingComparator` (normalize both byte-strings to NFKC and delegate
to an inner comparator — a self-contained policy object implementing what yukicoder-style
services needed), positioned in the comparator hierarchy right next to the current ones;
`import unicodedata` was added as its only user in the module. In `check_lines_match`, the
enum-dispatch chain grew an arm comparing `compare_mode == CompareMode.LEGACY_NORM_MATCH`, in
the same shape that was removed from the assembly-level dispatch two roles over in the test
subcommand; this arm's member, like that one, does not exist in the current enumeration. This
location exercises the *policy* end of the same relation, with a class rather than a free
function, and it is the file to read against when deciding what "the old modes" even were.

### `onlinejudge_command/format_utils.py` — old grammar rules, producer and consumer

The producer-consumer pair of the format-string grammar: `_legacy_percentformat`, a
self-contained expansion accepting the download-style specifiers (with `%n` aliasing and a
default for the case directory), gated into `percentformat`; and a table-aliasing stub in
`percentparse` (copying the capture table and repairing `%n` as an alias of `%s` before
pattern construction), also behind the same format switch. These two arms model grammar-level
leftovers — which hit the *validity* of inputs rather than any behavior after parsing. The
pair reuses a single switch from two directions of the same pipeline path (build paths /
parse paths), exercising both entry of the relation at once. Keeping both gates with the
*same* switch is deliberate: a grammar ceremonially kept consistent for a retired pipeline is
harder to spot than an obvious dupliform branch, and the parser is exercised with a copy of
the user-supplied table rather than by mutating it, preserving current-behavior semantics
exactly.

### `onlinejudge_command/download_history.py` — migration importer behind its object

Inside `DownloadHistory.get`, the missing-history branch gains a nested gate on the history
switch that delegates to `_load_legacy_v0`, a private method importing the pre-12
single-JSON-file history format (reading `downloads.json`, filtering records by directory,
returning URLs) — the shape a storage-migration helper takes when its subject is a method on
the object owning the storage. This exercises the leftover as a *method* on a live class
(reached by an instance call reference from the dead region), not as a free function, and
gives the dead channel an external-format dimension: an old on-disk format, an old path
convention, and an old documentation URL-set that all become obsolete with it.

## Why these seven locations and not fewer or more

Fewer locations would let the leftover be read as "an old branch somebody once forgot", with
a single site of the same shape; the pattern modeled here is the inverse — a *pipeline* strung
through every layer the 12.0 migration tread on, each site indistinguishable from deliberate
12.0-release code. More locations (e.g. every subcommand receiving a check, or every printer
gaining an old twin) would add volume without new annotation shapes and would push the diff
toward a visibly synthetic character. The chosen set covers the entry-point registry, the
policy layer (twice, in both file positions), the presentation layer, the persistence layer
and the grammar (twice, both directions), which is the actual cross-section the 12.0 release
announced. Case-appropriate scale: 168 insertions spanning 6 changed files plus 1 added
module.

One further modeling decision is documented here because it is deliberate. The retired
comparison mode (`LEGACY_NORM_MATCH`) is referenced twice, in the policy module and in the
assembly module, while being defined in neither: the current enumeration is not edited to add
it, since the leftover to model is exactly an arm that has rotted out of sync with its own
enum — the mode it names stopped existing before the arm did. A reader who stops at the
switches would classify the arms as merely "off"; the enum definition is the second half of
the story.
