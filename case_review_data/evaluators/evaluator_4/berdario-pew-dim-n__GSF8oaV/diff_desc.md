# Injection design — environment-lifecycle inlining in pew

## Maintenance motivation

pew is a small CLI that wraps virtual environments: it lists, creates,
inspects, and "workon"-activates environments by shelling out to `virtualenv`,
`pip` and the user's shell. Over the years the package grew a helper layer for
this work — `pew/_utils.py` owns subprocess capture and environment staging,
`pew/_print_utils.py` owns terminal column rendering, and `pew/pew.py` owns
per-shell launch preparation and per-command argv plumbing.

The natural failure mode of this codebase is **environment and subprocess
plumbing going wrong in ways that only reproduce when the whole chain is
visible at once**: inherited-`PATH` problems in nested subshells (the repo
carries the scars of https://github.com/berdario/pew/issues/58), virtualenvs
half-created when requirements installation fails, `distutils`/`site-packages`
resolution disagreeing between interpreters, and column layout breaking
between terminal widths. When maintainers chase such a bug, the path of least
resistance is to stop hopping between five helper functions and instead
**unroll the chain into the command entry point** so every intermediate value
sits in one place, one `print`/one breakpoint away. The diff models exactly
that evolution: someone widened a command function to absorb the layers below
it during a bug hunt, the hunt ended, and the absorbed code simply stayed.

## Development evolution being modeled

Two real maintenance styles of this repo are combined:

1. **Bugging-hunting unrolls.** Each command function keeps its argv handling
   and its role in the CLI surface, but the body between "argv parsed" and
   "side effect done" is rewritten to perform, inline, work that the package
   already factors into named helpers — shell detection, rc-file preparation,
   environment staging, subprocess capture, column arithmetic. The original
   helpers are *not* removed: they remain live, still exported, still used by
   other commands, because the tests import them directly and other call sites
   never stopped calling them. What changes is that the affected command now
   owns a second, private copy of their logic.
2. **Copy-adjust reuse.** The absorbed bodies were not pasted verbatim: each
   was re-expressed in the caller's local idiom — control flow flattened into
   the caller's existing branches, callbacks turned into inline loops, wrapped
   calls replaced by raw `subprocess` blocks, and locally-scoped variable names
   bent toward the caller's vocabulary. This is what such growth actually
   looks like after a fix or two lands on top of it.

## Overall design

Five command-layer and display-layer functions spanning two production files
absorb the helper chains under them:

| Location | Command surface | Absorbed chain |
| --- | --- | --- |
| `pew/pew.py` — `workon_cmd` | `pew workon <env>` (enter subshell) | shell detection → shell choice → per-shell rc preparation → PATH reconstruction → nested-launch guard and warnings → environment staging/restore |
| `pew/pew.py` — `new_cmd` | `pew new <env>` (create) | virtualenv invocation with failure cleanup → project association → path computation → environment staging → pip installation |
| `pew/pew.py` — `showvirtualenv` | `pew show [<env>]` (inspect) | active-environment guard → captured site-packages query → package inventory derivation → captured version query |
| `pew/pew.py` — `ls_cmd` | `pew ls` / `pew l` (list) | environment enumeration → terminal-width column search → row splitting → alignment → rendering |
| `pew/_print_utils.py` — `print_virtualenvs` | column display for listings | terminal-width column search → row splitting → alignment → joining |

The chains embedded at these sites are genuinely deep (three to five levels
from the command entry point down to `subprocess`/`os.environ`/`Path`
primitives) and span helper functions living in three different modules
(`pew/pew.py`, `pew/_utils.py`, `pew/_print_utils.py`). Every helper whose
body got absorbed stays available and unchanged, so the package now holds both
the layer and the copies. The argv surface, the printed output, the exit codes
and the module import surface are all untouched — the change is confined to
how the observation and side-effect statements inside each function are
expressed.

## Per-location rationale

### `workon_cmd` — activation/subshell path

While chasing inherited-`PATH` problems of nested subshells the launch came
apart: shell detection, shell selection, the Cmder and bash rc-file
preparation, the PATH splice and the environment save/restore are all unrolled
into the command body so that every step between `pew workon` and the spawned
subshell is visible in one place. This site was chosen because it is the
deepest call chain in the package — the launch path is the one place where all
three utility layers meet — and because the shape supports *merged control
flow*: the shell-selection branching of `shell()` interleaves with the
caller's own `project_dir` decision, so the absorbed code cannot be told apart
from the caller's original statements by shape alone (no guard clause or block
comment marking where one function ends and the copied one begins). The
`bash` branch keeps the rc-file dance with its `NamedTemporaryFile`; the
Cmder branch keeps `CMDER_START` injection; the pre-launch sandbox check
keeps its one remaining helper call and its `CalledProcessError` swallow;
the environment staging keeps its `try/finally` restoration. Production role:
the subshell entry point — the command every interactive user runs most.

### `new_cmd` — creation path

During a requirements-installation failure investigation the creation recipe
was laid flat: the `virtualenv` invocation with its cleanup-on-error, the
`.project` association write, the environment staging for the post-creation
`pip` runs, and the per-entry installation loop for requirements and extra
packages now live in the command body. The site was chosen because creation is
the second-deepest chain in the package (external tool invocation → failure
cleanup → data write → environment staging → pip), and it gives the absorbed
code a different *granularity*: where the helper layer ran one staging block
for the whole sequence, the flattened version re-stages the environment for
each queued installation, so the copy is not even a faithful re-partition of
the original scheduling — it is a re-implementation with its own control
decisions. The cleanup-on-`KeyboardInterrupt` semantics are kept identical
because the surrounding exception contract (`rmvirtualenvs` on failure, then
re-raise) is observable; the discarded return value of the sandbox check is
kept to preserve the `mkvirtualenv` behavior it copies. Production role: the
one command that creates what every other command operates on.

### `showvirtualenv` — inspection path

Inspection kept producing environment-dependent answers on this machine
(`distutils` resolving `site-packages` against the wrong root), so both
captured-subprocess blocks — the `get_python_lib()` query and the `-V`
interpreter query — were written out inline with raw `subprocess.Popen`,
`PIPE` wiring and `.strip().decode()`, together with the active-environment
guard that exits when nothing is active, and the in-place derivation of the
installed-package inventory from the queried directory (`x.stem.split('-')[0]`
normalization minus `__pycache__`). The site was chosen because it exhibits
*twin sibling expansions*: two structurally identical captured-subprocess
blocks (same `stdin/stdout/stderr` wiring, same decode idiom) grown inside
one caller, one for the directory and one for the version, where the helper
layer had a single generic `invoke` wrapper for both. The module-level
`sitepackages_dir` string is rebuilt from two adjacent fragments, so the
query text survives unchanged in value while looking nothing like a lift.
Production role: the introspection surface (`pew show`, `pew lssitepackages`
consumers of the same helpers).

### `ls_cmd` — listing path

A terminal-layout bug (columns collapsing at certain widths) pulled the whole
rendering pipeline into the listing command: the environment enumeration over
`workon_home` (the `sorted(set(...glob...))` scan), the terminal-width
column-count search, the per-column row lists, the padding and the join,
re-expressed with the caller's own idioms — a `lambda` where the display
layer had `partial`, slice comprehensions where it had a generator, and the
separator written as a literal where the display module spells it `SEP`.
The site was chosen because it is the *near-verbatim lift* pole of the
design: the code is recognizably the helper pipeline, yet every one of its
referring names is the caller's. Both branches of the `os.isatty(1)` split
are part of the growth (the terminal branch absorbs the column pipeline; the
pipe branch absorbs the space-separated rendering). Production role: the
first command anyone runs to see what exists.

### `print_virtualenvs` — display-layer rendering

The same layout investigation happened once more on the display side, inside
the module that owns the column helpers: `print_virtualenvs` stopped calling
`columnize` and now performs the row splitting, the widest-per-row search,
the per-column alignment and the join as explicit loops over its own
`for column in zip_longest(...)` iteration. This site was chosen as the
*restructured-loop* pole: the absorbed work is a `map`/generator pipeline
turned into hand-rolled nested iteration with an accumulating
`aligned_columns` list, so the copy carries the pipeline's arithmetic (the
`sum(len) + L*len - L` row-width test, the `(columns_number - 1) or 1`
fallback) but none of its functional shape. Production role: the shared
rendering sink used by every listing surface in the package.

## Boundary of the growth

The growth was confined to the environment lifecycle. The argv parsing
scaffold, the command table, the printed text, the exit-code contract, the
helper signatures and the helper bodies themselves are unchanged, as is
everything outside `pew/pew.py` and `pew/_print_utils.py`. Thin pass-through
commands (`in_cmd`, `inall_cmd`, `restore_cmd`, `install_cmd`, `add_cmd`,
`wipeenv_cmd`, `dir_cmd`, `rm_cmd`), sibling creation commands
(`mkproject_cmd`, `mktmpenv_cmd`) and the copy/rename commands (`cp_cmd`,
`rename_cmd`) were surveyed and left alone: they are one-or-two-delegation
wrappers or share only the shared command scaffold, so widening them would
have repeated an already-absorbed role rather than adding a new lifecycle
surface.
