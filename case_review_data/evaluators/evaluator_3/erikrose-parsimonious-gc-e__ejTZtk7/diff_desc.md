# Injection design record — the one-stop grammar coordinator

Repository: `erikrose/parsimonious` (a PEG parsing library) at commit
`eb79639859a9697a86c0992a045174a8856b5fb0`. Language: Python. The change
clustered here lives entirely inside the production package
(`parsimonious/grammar.py`, `parsimonious/expressions.py`,
`parsimonious/nodes.py`, `parsimonious/utils.py`).

## Maintenance motivation

`Grammar` is the object every parsimonious user holds, and over the library's
life it kept acquiring duties beyond the rule table it was born to be.
Byte-vs-str literal support made it meaningful to ask "is *this grammar*
internally consistent in the literals it uses?" — a grammar-scoped question,
not a per-visitor one. Custom-callable rules came in two calling conventions,
and telling them apart every time a callable was reused across rules cried out
for a remembered verdict rather than repeated introspection. Token grammars
raised a natural introspection question — which token types does this grammar
expect, so a user-supplied lexer can be checked against it — and a policy
question that is likewise a property of a token-mode grammar rather than of one
visit hook: regexes cannot match pre-lexed tokens. The `@rule` decorator, for
its part, had been carrying its rule strings loose on each method, and the
decorator's own code comment suggested gathering them into a registry so that
overriding a decorated method on a subclass would not strand the registered
syntax.

The maintenance direction this record models is: each of those needs was
answered by making `Grammar` the place where the answer lives. That is the
gravitation pattern of a coordinator object everyone already depends on — the
duties arrive one reasonable pull request at a time, each sounding sensible in
isolation, and nobody ever revisits where the resulting seams landed.

## Evolution modeled

The diff is written as a sequence of ordinary, separately-reviewable
maintenance steps rather than one grand rewrite:

1. Custom-callable wrapping moved from a module-level factory into the class,
   gaining a per-grammar classification cache (callables can be reused by
   several rules; method descriptors are unwrapped so `@staticmethod`
   callables keep working).
2. String-literal evaluation and the byte/str same-type record moved from the
   general utilities module and the rule visitor's own instance state into the
   grammar, and every literal production was routed through it so the record
   cannot be bypassed mid-construction.
3. Token-grammar bookkeeping (a ledger of token-type spellings) and the
   token-mode regex refusal moved into the grammar, with the token rule
   visitor delegating both.
4. Rule-string registration for `@rule`-decorated visitor methods moved into a
   class-level registry, and the metaclass's default-grammar assembly was
   delegated to a classmethod pair.
5. With their last production callers gone, the two helper functions were
   dropped from their donor modules, which were trimmed back to the surface
   other code actually shares.

## Overall design

`Grammar` remains subclassable and dict-like; nothing about the public type
surface changes. What changes is where decisions live:

* the class gains registration bookkeeping initialized at the top of
  `__init__`, one region per duty;
* the rule-tree builders (`RuleVisitor`, `TokenRuleVisitor`) accept and store
  the grammar being built and route literal, token and regex decisions through
  it; a builder that arrives with no grammar lazily spins up a blank one;
* the token grammar and the bootstrapping grammar hand their own instances to
  the builders, so subgrammars and the meta-grammar use the same facilities;
* the decorator and the visitor metaclass delegate tagging and assembly to the
  grammar;
* the two absorbed helpers keep their docstrings, return-to-node behavior and
  message texts, since those are part of the contract callers experience.

## Per-cluster rationale

### `parsimonious/expressions.py` — the custom-callable factory absorbed

* **What changed:** the module-level `expression(callable, rule_name, grammar)`
  factory (and the `AdHocExpression` class it defined per rule) is deleted;
  the `getfullargspec` import goes with it.
* **Why here and this form:** the factory had exactly one production caller —
  the grammar constructor — and its two variant behaviors (2-arg vs 5-arg
  callables, descriptor unwrapping) needed to consult per-grammar state, which
  a free function cannot own. Folding it into the class it served removed a
  cross-module hop while keeping the callable contract intact.
* **Production role:** custom rules passed as kwargs or in rule maps become
  `Expression` nodes that call user code at match time, transmuting int/tuple
  results into `Node`s; `is_callable` stays exported here unchanged because
  callers outside this story still use it.

### `parsimonious/utils.py` — the literal evaluator absorbed

* **What changed:** `evaluate_string` (and its `import ast`) is deleted from
  the general-utilities module.
* **Why here and this form:** after the literal-production funneling below,
  this helper had a single production caller and a grammar-specific meaning —
  it defines what a quoted spelling *in a rule language* means, including
  `b"..."` and `r"..."` forms. Its new home as a grammar method keeps the
  docstring and `ast.literal_eval` behavior verbatim; the utilities module
  returns to holding only the pieces that truly do not depend on other parts
  of the library.
* **Production role:** turns rule-text string spellings into Python values;
  `Token`, kept here, still backs token grammars.

### `parsimonious/grammar.py` — the coordinator grows

* **Imports:** the module gains `ast`, `getfullargspec`, `ismethod`,
  `ismethoddescriptor`, `Node`, `Expression` and `version_info` (the sort key
  selection moved here from nodes.py) and stops importing the two absorbed
  helpers. Each import lives where its last user now lives.
* **Class docstring:** a short paragraph documents the broader coordinator
  role alongside the existing usage examples. Drift between a class's story
  and its reality is its own maintenance cost, so the story is updated in the
  same change.
* **`__init__`:** construction now opens by preparing the bookkeeping this
  class keeps handy — the custom-rule-form cache, the literal-type record and
  the token-type ledger — before rules are resolved. The custom-rule wrapping
  call switches from the deleted factory to the in-class method. Ordering is
  chosen so every later step (wrapping callables, building rules) finds its
  registries ready.
* **Custom-callable cluster** (`_init_custom_rule_forms`, `_custom_rule_form`,
  `_wrap_custom_rule`): classification is remembered per callable object so a
  callable shared by several rules is inspected once; bad arity still raises
  the same `RuntimeError` at construction time; `@staticmethod` descriptors
  are unwrapped exactly as before. `_wrap_custom_rule` is the old factory
  body, now closing over the owning grammar so five-argument callables can
  still reach sibling rules and report errors through it.
* **String-literal cluster** (`_init_literal_registry`,
  `evaluate_rule_string`, `make_literal`): the same-type record that used to
  sit on a rule visitor is now grammar state, so it observes every literal of
  the whole construction instead of one visitation. Refusal keeps its
  `BadGrammar` text and the accumulate-then-refuse shape: the message names
  both offending literal spellings and their types.
* **Token cluster** (`_init_token_types`, `make_token_matcher`,
  `check_token_regex_support`): the ledger collects the token-type spellings
  a grammar's rules actually mention, which is the prerequisite for telling a
  user their lexer produces the wrong (or unused) types; matcher construction
  and the regex refusal read as two facilities of the same token-mode story,
  so they live together. The refusal still raises at rule-tree construction
  time with the original message.
* **Rule-method registry** (`_rule_method_syntaxes` class attribute,
  `tag_rule_method`, `from_rule_methods`): the decorator registers rule
  strings keyed by the decorated method itself, which is the registry shape
  the decorator's old comment asked for — subclass overrides no longer strand
  syntax strings. `from_rule_methods` is the assembly code that used to be
  inlined in the metaclass, kept as a classmethod so subclass replacement of
  the constructor still assembles grammar objects of the right kind, and so
  the first-decorated-by-source-line default-rule choice (including its
  Python-2/3-aware `func_code`/`__code__` sort key) survives verbatim.
* **Visitor threading** — `Grammar._expressions_from_rules`,
  `TokenGrammar._expressions_from_rules`,
  `BootstrappingGrammar._expressions_from_rules`: each spawn site now passes
  `self` into the rule visitor it creates. The bootstrapping grammar passing
  itself means the meta-grammar's own literals are produced through its own
  record — the grammar language described by the grammar language exercises
  the same facilities user grammars do.
* **`RuleVisitor.__init__`:** takes an optional grammar and stores it,
  replacing the visitor's own literal-typing attribute. The old no-argument
  construction stays valid.
* **`RuleVisitor.visit_spaceless_literal`:** literal construction is delegated
  to the grammar; a visitor that turns up without one (the builder can be
  used stand-alone) lazily attaches a blank grammar so its literals still get
  typed consistently. The lazy attach exists because the bare-constructor
  path is part of the builder's usable surface.
* **`TokenRuleVisitor` hook overrides:** the token literal hook and the
  regex hook become thin delegations to the token facilities described above;
  the parse hooks keep their position and signatures, so the same node is
  produced at the same moment in the visitation with the same error surface.

### `parsimonious/nodes.py` — the registration path delegates

* **`RuleDecoratorMeta.__new__`:** selection of decorated methods still relies
  on the `_rule` marker, but assembly of the default grammar now goes through
  the grammar classmethod instead of inlined sort-and-join code; the metaclass
  keeps its local `Grammar` import (the established circular-import dodge —
  grammar.py imports nodes.py at module level).
* **The `rule` decorator:** tags through the same classmethod pair rather than
  writing the marker directly, replacing the comment that only proposed the
  registry; the marker attribute is still set so metaclass selection is
  unchanged.
* **`from sys import version_info`, exc_info:** only the sort-key selection
  needed `version_info`; it moved with the code that uses it. `exc_info`
  stays for node visitation error reporting.

## Boundary choices

Some candidates were deliberately left alone: `Node.prettily` and the
pretty-printing shapes are match-side output, and reaching them would require
threading a grammar handle through expression objects that deliberately do not
carry one. Match-time error reporting in `exceptions.py` is initiated by
expressions during matching, not construction. `NodeVisitor` itself is a
generic traversal shell one concern wide. The `Expression` hierarchy already
coheres on what it means to match; adding construction duties there would
have mixed two stories that presently keep cleanly apart.
