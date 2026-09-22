# Environment lifecycle: commands have stopped using their helper layer

Over the past rounds of debugging in this python CLI for virtual environments
(nested-subshell `PATH` trouble, environments half-created when a requirements
install fails, `site-packages` resolving against the wrong interpreter, column
layout breaking between terminal widths), the big command handlers grew to
"show every step in one place" — and stayed that way. We now keep fixing
things in the helpers, and the flows that re-derived those steps by hand don't
pick the fixes up, so lately several of us have patched the same logic twice.

Reading the flows now, the same steps are spelled out twice: once in the
reusable helpers the package already exposes, and once re-written privately
inside a command body. The affected behavior is the whole environment
lifecycle:

- entering a subshell inside an environment (how the shell is detected and
  chosen, rc handling for bash-style and Windows shells, the `PATH` handed to
  the child, saving and restoring the outer environment around the launch,
  and the messages printed when starting/stopping);
- creating an environment, including installing its requirements and any
  extra packages, and what gets cleaned up when creation or installation
  fails;
- inspecting an environment (the guard when no environment is active, asking
  the environment's interpreter where its site-packages directory lives,
  deriving what is installed from it, and capturing which interpreter it is);
- listing environments, both into a terminal and when piped;
- the column arrangement the listings are rendered with (widest-per-row
  search, row splitting, padding, joining).

The helper functions behind those steps still exist, still work, and are
still used or imported elsewhere — but the commands that grew during the bug
hunts no longer call them: they re-perform the work inline, in several
different hand-rolled shapes (some close to the original helpers, some
rewritten as loops or raw subprocess wiring with their own local idioms).

Please work through the environment lifecycle of the CLI and restore
delegation: wherever a command re-implements steps that the package already
factors into a reusable helper, make it call that helper again, so each step
has a single implementation lived at by all flows. This is repository-level
work — investigate the lifecycle thoroughly and address every place the
pattern occurs (activation, creation, inspection, listing, and the shared
column display), not just the most visible one.

Keep everything observable exactly as it is: same CLI text and output
arrangement, same exit codes, same behavior on creation/installation failure,
same environment restoration after a subshell or command run, and do not
drop, rename, or change the signatures of the existing helpers — other
commands and the test suite still rely on them (the display helpers are
imported by the tests directly and must stay importable and behave the
same).
