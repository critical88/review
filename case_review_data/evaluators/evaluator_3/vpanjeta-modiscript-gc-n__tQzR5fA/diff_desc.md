# Injection design record — ModiScript front-end responsibility centralization

## Repository context

ModiScript is a small Python implementation of a satirical Hindi-English
scripting language ("Modi"). The production core is the `modiscript` package:
`utils.py` (token code tables, word-spelling aliases, the shared error type),
`lexer.py` (the front-end that turns program text into typed lexemes and
enforces the required program opening/closing protocol), `parser.py` (the
back-end that turns the token stream into Python AST), and `api.py` (the
`ModiScript` driver that glues the two halves together and offers the
`execute` entry point used by the `MODI` command line and the Flask
playground in `website/web.py`). Recent project activity ("Lexer optimisation
for repeated character fetch", language examples such as the voter-age and
election scripts) has concentrated on the front-end, which is where hot-fixes
keep landing.

## Realistic maintenance motivation

The work models a familiar failure mode of a small, hobby-scale language
runtime under deadline pressure: every new convenience lands in the file the
maintainer already has open. Three needs arrived one after another.

1. The hosted playground and the CLI both wanted richer diagnostics for
   beginner programs, which meant keeping the debug dump files (the token log
   and the parse-tree log) in step with front-end changes. Instead of a
   reporting component, the dump code was pushed down into the front-end "so
   the person touching tokens owns the reports", and the driver was reduced to
   a caller.
2. Vocabulary corrections (spelling aliases such as `nahin -> nahi`,
   `bhayyo -> bhaiyo`) had previously lived in a module table next to the
   one-use static helper. To let alternative spellings be taught per run, the
   table was copied onto the front-end object as editable per-run state,
   again inside the same class.
3. The same drift happened to the program boundary words ("mitrooon" ...
   "acche din aa gaye"): local constants in `analyze` were promoted to a
   per-object protocol record, and file/string ingestion was reshaped into
   staged per-object state with the class holding both a path and a staged
   line buffer.

The result reads exactly like the repository's real commit history: a class
that used to form tokens now also loads programs, keeps spelling/correction
state, describes the required program protocol, and writes the CLI's debug
reports.

## Overall design of the change

The diff expands the token-producing class of the language front-end so that
its own behavior methods partition into several mutually disconnected
instance-state groups, while the observable contract of the repository is
left intact:

* every packaged export keeps its name and semantics (`Lexer`,
  `Lexer.analyze`, `Lexer.lexeme`, `Lexer.normalize`, `Lexer._is_var`,
  `Parser`, `ModiScript.execute`, `ErrorHandler`, `LEX`, `WORDS`, and the
  module import paths used by the CLI, the website, and callers);
* the constructor gains only an optional trailing argument, so existing
  two-argument constructions keep working, and the website's code-mode
  execution path is untouched in effect;
* token output, error codes, argument tuples, and message strings for every
  recorded behavior (marker enforcement, vocabulary corrections, token codes
  and offsets, debug dumps) are produced by the same logic as before, moved
  but unchanged in effect.

Two production files change. `modiscript/lexer.py` absorbs the extra duties
into the token-producing class; `modiscript/api.py` gives up its debug-file
writing and becomes a driver that delegates to the expanded front-end
surface. The parser, the utility tables, the CLI entry script, and the
website server are not part of the change.

## What changed, location by location

### 1. `modiscript/lexer.py` — constructor and absorbed duty groups

**Source provisioning (absorbed into the class).**
`Lexer.__init__` previously read the program file inline when constructed in
filename mode, or split an inline string, assigning `self.contents` directly.
The change routes both ingestion modes through two new behavior methods,
`_read_source(path)` and `_inline_source(value)`, which stage the lowered
program lines on new per-object state (`self._source_lines`, plus
`self._source_path` in the file case) that the constructor then copies into
`self.contents`. This location was chosen because ingestion is the first thing
the front-end does, so a maintainer growing the file naturally re-shapes it
first; staging the lines (instead of building `contents` directly) models the
"keep the raw material around for later features" step that creates owning
state few other methods share. Production role: how program text enters the
pipeline from disk or from a string.

**Runtime spelling registry (absorbed into the class).**
The word-correction cascade previously consulted the packaged `WORDS` alias
table through a static helper. The change keeps the static helper for
compatibility but adds per-run state alongside it: `_collect_word_fixes()`
seeds a new `self._word_fixes` dictionary from the packaged table at
construction, and `_correction_for(word)` consults that per-run copy before
falling back to the static normalization, with the token loop now calling the
instance lookup instead of the static helper. The lookup order (seeded alias
first, pattern cascade second) reproduces the previous alias-first,
pattern-fallback behavior for every input. This location was chosen because
the vocabulary belongs to the front-end lexically, and an editable per-run
copy placed on the token producer is the typical "configuration as instance
state" drift that ends up buried in a busy class. Production role: which
alternate spellings count as which language words during a run.

**Program boundary protocol (promoted to instance state).**
`analyze` previously validated the opening word with a literal and checked
the closing phrase against a locally built list. The change moves both into a
per-object protocol record `self._program_protocol` (opening word + closing
phrase) and two methods, `_expect_opening(lexeme)` and `_expect_closing(lexemes)`,
raising the same errors with the same codes when the marker rules are
violated. The closing check still consumes tokens and phrase words pairwise
in the same order, preserving the error behavior for truncated and malformed
programs. This location was chosen because envelope compliance is the
front-end's public gate; promoting it from method locals to object state
mirrors the real drift toward a class describing "how a program must be
packaged" while also forming tokens. Production role: the required program
marker rules of the language.

**Debug report authoring (absorbed from the driver).**
The `ModiScript` driver previously owned writing the debug artifacts: the
token log `.txt` (joined tokens) and the parse-tree log `.py` (an `ast.dump`)
written next to the analyzed file when the debug switch was on for filename
runs. The change moves that duty into the front-end as `record_lexemes(...)`
and `record_parse_tree(...)`, guarded by per-object state established at
construction (`self.debug`, a filename-mode flag `self._dump_ready`, and a
trace stem `self._trace_stem` derived the same way the driver used to derive
it). The token log is still written as soon as lexing finishes, and the tree
log after parsing, so the artifacts, their trigger condition, their paths,
and their content are produced by the same behavior as before. This location
and shape were chosen deliberately: taking a duty *from another owner* into
the busiest class is the defining texture of this kind of decay, and writing
parse reports from the token producer is precisely the beginner-level
ownership mistake the case is about. Production role: the CLI's `-d/--debug`
evidence files.

**Tokenization core (kept as the class's home ground).**
The token loop, the stack helpers (`_push`, `on_top`, `pop`), `analyze`, and
the static helpers stay in place, with one mechanical adaptation: word
classification consults the per-run registry through `_correction_for` rather
than the static helper directly. Keeping the original duty intact was chosen
so the case stays a centralization problem rather than a rewrite problem.

### 2. `modiscript/lexer.py` — constructor as wiring point

The constructor is reshaped into the wiring point for all five groups: stack
and clear flags, ingestion (filename or inline), the spelling registry seed,
the protocol record, and the report state. Only the ingestion and registry
seeding require more than assignments. The constructor still accepts the
previous two positional arguments and assigns `contents`, `stack`, and
`clear` with the same meanings and order of operations observable to
constructors of the class.

### 3. `modiscript/api.py` — the driver becomes a caller

`ModiScript._compile_file` sheds its dump-writing blocks and instead builds
the front-end with its debug switch, asks it for the token stream, and calls
the two report methods around parsing. `ModiScript.execute` and the public
constructor of the driver are unchanged. This location was chosen as the
second participant because it is the only caller of the front-end: moving its
reporting duty out and re-wiring it through the expanded front-end surface is
what makes the absorption load-bearing in production rather than speculative.
Its production role is the pipeline driver used by both the CLI and the
website.

## Deliberate structural variation

The absorbed groups are intentionally shaped differently from one another so
that no single template repeats: one group arrives from another owner (report
authoring), two are promotions of locals or of a module table into per-object
state (protocol record, spelling registry), and one reshapes existing
constructor work into staged owning state (provisioning). The token core and
the static helper surface are left in their original form as the class's
legitimate home ground, so the file reads like a class that grew a variety of
unrelated second jobs, not like one repeated edit. Method naming follows the
class's existing private-underscore convention with two public report
methods, reflecting that they form the new surface the driver calls.

## Scope explored and excluded

Absorbing the AST-building back-end into the front-end was rejected because
`Parser` is a pinned public export whose behavior callers rely on; folding
error-type formatting or the CLI's `usage()` into the front-end was rejected
because the error contract is shared with the driver and the CLI; moving the
packaged `WORDS` table itself was rejected because the packaged language
tables are part of the module's stable surface; and reworking
`website/web.py` was rejected because the playground only consumes the
driver's public `execute` and deriving a served change there would not be
exercisable through the repository's behavior contract. The change therefore
concentrates on the front-end and its driver, which is where this evolution
genuinely happens.
