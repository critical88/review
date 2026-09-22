# Refactoring task: untangle the grammar-construction subsystem

## Where this comes from

`parsimonious` builds expression trees from a small rule language: `Grammar`
stores rules as an ordered mapping and dispatches `parse()`/`match()` through
a default rule; `TokenGrammar` accepts pre-lexed tokens; a two-level
bootstrapping grammar defines the rule language itself; `RuleVisitor` and its
token variant walk a parsed rule tree and produce `Expression` objects; the
`@rule` decorator stitches rule syntaxes onto `NodeVisitor` methods.

Review of the current code shows that essentially every construction-time duty
of the library now lives inside the `Grammar` class in
`parsimonious/grammar.py`, on top of its founding job of storing rules and
dispatching through the default one:

* it classifies custom callables (2-argument vs 5-argument form), remembers
  that verdict per callable, and wraps each into an ad-hoc expression class;
* it evaluates every rule-language string literal, records their type, and
  refuses grammars that mix byte and str literals;
* it constructs token matchers and accumulates the token types a token
  grammar mentions; it also owns the policy that token grammars cannot use
  regexes;
* it keeps the class-level registry of `@rule` syntaxes and assembles the
  default grammar for decorated visitor classes.

Each of those duties maintains its own state, reachable only inside that
slice of the class, and the rule-tree builders, the token grammar, the
bootstrapping grammar, the `@rule` decorator and the visitor metaclass are
all threaded through the same object to obtain those services. Nothing about
the public object model forced this: the absorbed duties are ordinary,
separable facilities with production callers that could reach them through
several other seams.

## The maintenance problem

These duties change for different reasons: a new literal form, a new calling
convention for custom rules, token-introspection requests, or decorator
registration semantics each send a maintainer into the same large class, where
one must reconstruct which instance state a method owns and in what order the
constructor has to prepare it before rules can be resolved. The class has
become the place where unrelated changes land next to each other; any two of
its slices can be broken by an edit aimed at a third.

## What to do

Restore single-purpose ownership across the grammar-construction subsystem.
Analyze the duties listed above, decide where each one genuinely belongs (a
fitting owner reachable from existing call paths), and move the logic and its
state there, re-threading the builders, the token and bootstrap construction
paths, and the decorator/registration machinery accordingly. Address all of
these duties, not just the most visible one, and do more than relocate the
whole cluster into another single owner — the goal is that a change to any
one duty stops requiring reading and disturbing the others.

Keep the design free to take any reasonable shape: new homes may be classes,
small helper objects, or restored module-level facilities, as long as each
duty has a well-scoped owner and the observable behavior below is preserved.

## Compatibility boundary

The following behaviors are part of the library's contract and must come out
identical:

1. **Rule grammars.** `Grammar('...')` builds the same rule map, default-rule
   choice (first rule; last declaration wins on a redefined name) and
   expression objects; `g['rule'].parse(...)`, `g.parse(...)`, `g.match(...)`,
   `.default(...)`, and rule-string round-tripping through `str(g)` keep their
   semantics; parse errors propagate unchanged.
2. **String literals.** Python escaping semantics, including `b"..."` byte
   literals and `r"..."` raw spellings, work as documented; mixing byte and
   str literals in one grammar still raises `BadGrammar` with the message
   naming both literals and their types; all-str and all-byte grammars
   construct normally.
3. **Custom callable rules.** Callables taking `(text, pos)` and callables
   taking `(text, pos, cache, error, grammar)` both work, with the grammar
   passed to the long form able to reach sibling rules; returning an int, a
   tuple, a `Node` or `None` behaves as documented; bad arity still raises
   the same `RuntimeError` at grammar construction time; callables shared by
   several rules keep working, as do `@staticmethod` supplied callables.
4. **Token grammars.** `TokenGrammar('...')` builds token matchers that match
   `Token` objects by `type` and parse token sequences as before; regex usage
   in a token grammar still fails at construction time with the identical
   `BadGrammar` message.
5. **Decorator machinery.** `@rule('~"[0-9]"')`-style decoration still mounts
   the rule on the method and a decorated `NodeVisitor` subclass still gets
   its default grammar, choosing the first-decorated rule by source line;
   overriding a decorated method on a subclass does not strand the registered
   syntax. `NodeVisitor` classes without `@rule` methods behave as before.
6. **The self-hosted rule language.** The bootstrapping grammar must still be
   assembled by the same machinery at import time, so the rule language that
   parses rule descriptions keeps working unchanged.
7. **Public surface.** All existing imports from `parsimonious.nodes`,
   `parsimonious.utils`, `parsimonious.expressions` and
   `parsimonious.grammar` (for example `NodeVisitor`, `Node`, `rule`, `Token`,
   `is_callable`, `RuleVisitor`, `Grammar`, `TokenGrammar`, `LazyReference`)
   continue to work, including constructing a rule visitor with no arguments;
   no public name may disappear.
