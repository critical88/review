# Refactor: adding an operator should not require a trek through the whole pipeline

## The problem I ran into

We maintain this Go expression-evaluation library. Over the last while, several of us
have touched different parts of the evaluation pipeline — the code that turns an
expression string into tokens, the pass that folds constant sub-expressions, the code
that turns a token stream into a plan of evaluation stages, and the code that renders a
planned expression as SQL. Each change made its own little corner easier to work on.

The bill came due when I tried to add one new comparison operator last week. Before my
branch was done, I had taught the same spelling list — "which operator spellings this
library understands, and what each one is" — to a handful of different places, and I
still was not confident I had found them all. At some point the library stopped having
one authoritative notion of its operator vocabulary: each consumer of the grammar now
keeps its own local copy of the spelling list, so any change to the set of operators
means the same small edit in several places, and missing one produces a library that
quietly disagrees with itself.

The copies have already started to drift. One concrete example: the textual `in`
comparator is treated as part of the comparator list in some places, while other
places never look it up through that list at all, so "which comparators exist" has
no single answer in the codebase today.

A related annoyance while fixing a separate SQL bug: to answer "can this operator be
precompiled against a constant right-hand side?" the folding pass now consults its own
private notion of the comparator list rather than one shared classification.

## What I want

Please investigate everywhere the evaluation pipeline consumes the operator vocabulary —
starting where characters are classified into tokens, then following every consumer of
that classification through planning and output rendering — and consolidate the
operator spellings and their classification into one shared, authoritative home that
all those consumers consult, instead of each keeping its own copy. Remove the private
copies rather than leaving them delegating in place; stray remnants of the duplicated
lists are exactly the maintenance trap I am trying to eliminate.

The goal is that a language-level change — adding a spelling, removing one, or
reclassifying one — requires editing that one home, plus whatever genuinely
stage-specific handling necessarily knows about individual operators.

Keep the local distinctions that are real, though: where a stage intentionally deals
with only a subset of the vocabulary (for example the precompile-able regex operators,
or a single precedence level's accepted spellings), it should obtain that subset from
the shared classification rather than restating the spellings.

## What must not change

This is a restructuring, not a behavior change:

- Every expression that evaluates today evaluates to the same result, with the same
  precedence, the same type-checking, the same error messages, and the same formatting.
- The SQL rendering of any expression is byte-for-byte identical, including the
  dialect spellings it uses for logical, comparison, regex, exponent, and modulus
  operators, and its refusal to translate ternary operators.
- Constant folding still precompiles only the regex comparators against constant
  right-hand sides.
- Unary `-`, `!`, and `~` continue to be classified as prefix operators when the
  current lexer state allows a prefix and as binary modifiers otherwise; the textual
  `in`/`IN` comparator keeps working exactly as it does, including in SQL output.
- The public API of the package is unchanged.
- The repository's full test suite passes as-is; no test expectations are modified.
