# Clean up permanently shelved experimental plumbing in the expression pipeline

## Maintainer observation

govaluate builds still carry the wiring for several experimental capabilities that
were prototyped, hooked into the runtime, and then shelved before ever being
enabled in a release. Each hook is guarded by a build-configuration constant that
is declared once at package scope next to the package's other compile-time
constants, and the values shipped in this build permanently disable every one of
those capabilities. That means the guarded regions are unreachable in every
supported build configuration: the code looks like ordinary pipeline logic, reads
as if it could run for the right input or configuration, and pays maintenance
costs in exactly the phases maintainers read most — but no runtime path,
registration, or supported configuration can ever reach it.

The shelved experiments cover, at minimum: a tracing mode that would interpose in
the recursive evaluation of the stage tree; an extended operator-typing regime that
would substitute itself during stage planning; a multi-dialect variant of the SQL
translation output; a stricter identifier-validation diagnostic applied while
tokenizing; and an alternate numeric-promotion route applied when parameters are
coerced during access. Any of these dormant hooks can also appear in more than one
form or spot within its phase, so the phase — not a single known site — is the
unit of investigation.

## Where to look

The dormant plumbing is spread across the main pipeline phases:

- construction and recursive evaluation of the expression's stage tree;
- stage planning, in particular the selection of type-check rules per operator;
- token-to-SQL translation for serializable expressions;
- token scanning and identifier validation in the lexer;
- parameter access, sanitization, and coercion of values supplied to expressions.

The toggles gating all of this are compile-time constant declarations, so
deciding reachability is a matter of what the compiler can already prove about
each guard, not of running the code.

## What to do

Investigate the phases above, find every region that this build's configuration
constants make permanently unreachable, and retire the shelved capabilities
completely: each dormant region goes, together with the guard that only exists to
protect it and any configuration declaration or alternative arm that existed only
to feed it. Where a dormant branch sits interleaved with a live default path, the
genuinely used path must be preserved exactly — removing the dormant branch may
not change what the surrounding function returns for any value it can actually
receive. The cleanup is complete when the affected phases contain no code whose
entry is decided by a permanently disabled switch.

This is a removal task, not a re-enablement task: the shelved capabilities stay
shelved, and no dormant region may be replaced with a different dormant region
(for example, a removed branch may not be re-gated behind a new constant, and no
new configuration knob may be introduced).

## Behavior and compatibility boundary

- Observable behavior must be identical before and after the cleanup: the same
  expressions parse, evaluate, and error as they do now; the same parameter types
  are accepted and coerced with the same results; the same expressions remain
  serializable to SQL with the same output text.
- The public API of the package stays as it is: the constructors, evaluation,
  token, variable, and SQL-conversion methods callers rely on keep their names,
  signatures, and semantics, and the package's import path is unchanged.
- Only code that is unreachable in every supported build configuration because a
  compile-time constant disables it is in scope. Runtime-conditional code —
  branches chosen by parameter values, expression content, operator kinds, or
  short-circuiting behavior — is data-dependent, not dead, and must be left
  intact even when it looks rarely taken.
- The full test suite, including the batteries that only run when the repository's
  torture-test environment variable is set, continues to pass without
  modification. No test changes are wanted or expected as part of this work.

## Out of scope

- Adding new features, knobs, or toggles, or reviving any shelved capability.
- Restructuring, renaming, or reformatting live pipeline logic beyond what the
  removal itself requires.
- Any change to files outside the phases listed above.
