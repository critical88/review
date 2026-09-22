# Remove the retired pre-12 pipeline paths from the `oj` command

## Maintainer observation

`online-judge-tools` 12.0 was a coordinated break: at the same release we replaced the
rendering used to present the results of local tests, the modes used to compare the user's
output with the expected output, the rules used to expand and parse format strings, the
accepted names of subcommands, and the on-disk format of the download history. To let users
migrate gradually, that release kept the old behaviors selectable through a small
compatibility layer: a single pinned level constant consulted by a handful of boolean
selector functions, one per replaced behavior, spread across the command package.

The grace period ended one milestone ago. The level is not configurable and sits at its
"fully retired" value, so every selector always answers "off". The retired machinery is
still intact in the tree — and it has started to rot: at least one of the old comparison
arms selects by a mode name that the current comparison-mode enumeration no longer defines,
so that arm cannot match any input the parser can even represent.

## What to do

Delete the whole retired pipeline, not its visible symptoms. Concretely:

1. Remove the switches themselves — the selector functions and the pinned level constant
   guiding them, together with whatever module exists solely to host them.
2. Remove every code region that only the disabled switches can select, across all the
   behavior areas named above: the old subcommand-name arm(s) in the dispatch table, the
   old comparison arm(s) in the output-matching assembly, the old rendering path(s) in the
   diff/verdict presentation, the old format-string rule(s) in both the expansion and the
   parsing directions, and the old download-history import.
3. Remove the implementations those regions were keeping alive: the old renderer(s) and
   their private helpers, the old comparator class(es), the old format-string expander(s),
   and the old history reader(s) — including members defined on classes, and anything
   transitively reachable only from them.
4. Remove the references that exist only to serve what you delete: import statements added
   for the old pipeline, and any selection by a mode value that no longer exists in its
   enumeration — do NOT reintroduce retired values into the enumerations.
5. Do not replace the retired paths with a new "always off" option, knob, or constant; the
   correct end state is that no trace of the old pipeline remains, not that it became
   configurable again.

## Scope boundary

Work inside the command implementation package of this repository (the `oj` CLI's own
Python package). The separate `onlinejudge` service-library package (problem/service
abstractions used by the command) is out of scope. Production code only: do not modify
tests, documentation, or packaging files.

## What must stay stable

The current (12.x) user-visible behavior is already correct and must not change:

- the subcommand table of the argparse front end, including aliases and help text;
- the four selectable comparison modes (`exact-match`, `crlf-insensitive-exact-match`,
  `ignore-spaces`, `ignore-spaces-and-newlines`) and the CLI wording that exposes them;
- the `%s`/`%e` specifier grammar of the local-testing format strings;
- the JSONL download-history format, and the behavior that a not-yet-existing history
  reads as empty;
- the current colored/tokenized presentation of test results in all its display modes;
- exit codes, log headers, and the update notice.

The complete test suite of the repository must keep passing exactly as it does today, with
no new failures and no new skips. When you are done, nothing that a current user can
observe should differ except the absence of the dead machinery.
