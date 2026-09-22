# Injection design record — govaluate operator vocabulary

## Maintenance motivation

govaluate's grammar is its product. The pinned revision grew its operator set the way
small libraries usually do: a textual `in` comparator for collections, `~`/`!`/`-` as
prefixes, `??` for coalescing, `**` for exponentiation, and bitwise operators for parity
with other expression languages. Every one of those additions touched the same conceptual
list — "which spellings exist, and what each one means" — and, historically, that list
lived in exactly one place: the package-level spelling-to-symbol tables next to the
`OperatorSymbol` definition.

The motivation modeled here is that of a library entering a phase where each pipeline
stage is maintained semi-independently. The expression path has four genuinely different
consumers of operator spellings — the lexer that turns characters into typed tokens, the
constant-folding pass that precompiles right-hand operands of regex comparators, the
precedence planner that decides which spellings a given phase accepts, and the SQL
translation layer that maps spellings onto a SQL dialect. When contributors work inside
one stage, the smallest reviewable change is a local one; a package-level registry owned
by nobody in particular is the natural thing to stop consulting, especially once each
stage wants slightly different treatment of the same spellings.

## Evolution being modeled

The change models ordinary piecemeal development, not a single rewrite:

1. A lexer contributor makes tokenization self-contained by giving `readToken` its own
   spelling tables, so lexer fixes no longer depend on definitions outside the lexing
   code. This is a classic "local variables instead of distant globals" change that
   reviewers happily accept for a hot function.
2. A performance contributor makes constant folding independent of the full comparator
   grammar, matching local needs to local knowledge: the pass only cares about the two
   precompilable regex comparators, so it keeps exactly those two spellings.
3. A planning contributor gives each precedence phase its own acceptable-spelling table,
   arguing that precedence tweaks should be auditable within `stagePlanner.go` alone —
   the phases' sets genuinely differ (additive has two spellings, multiplicative three,
   the comparator phase nine), so per-phase tables read naturally.
4. A translation contributor, fixing a SQL-rounding bug, follows the same local-first
   style in `findNextSQLString` so dialect handling can be adjusted without coordinating
   with the rest of the pipeline.
5. Once no consumer remains, the shared package registry in `OperatorSymbol.go` is dead
   weight and is removed as cleanup, alongside its doc comments.

Each step is the kind of change a real repository absorbs; their cumulative effect only
shows up the next time someone adds an operator.

## Overall design

The four consumer stages each receive self-contained, function-local string-keyed tables
mapping operator spellings to `OperatorSymbol` values, and the single shared registry is
removed. Lookups keep working exactly as before, so evaluation semantics are untouched;
what changes is where operator classification knowledge has to be edited. The table
copies are deliberately not photostats: the copies at each site differ in scope
(function-local in the lexer and translator, phase-local in the planner) and in content
(the lexer's comparator set has eight spellings while the planner's and translator's have
nine), because the stages they serve genuinely needed different subsets in the modeled
history. Three unrelated shapes were left untouched on purpose: single-entry inline
literals (`"&&"`, `"||"`, `","`, the exponent phase) already existed as inline one-entry
maps on the pinned revision, so they stay inline; symbol-keyed dispatch (precedence
lookup, stage-symbol and type-check selection, `OperatorSymbol.String` rendering) is
keyed by enum value, not by spelling, and was not part of the string-keyed relation.

## Per-cluster rationale

### `OperatorSymbol.go` — the registry is dissolved

The ten package-level spelling tables (`comparatorSymbols`, `logicalSymbols`,
`bitwiseSymbols`, `bitwiseShiftSymbols`, `additiveSymbols`, `multiplicativeSymbols`,
`exponentialSymbols`, `prefixSymbols`, `ternarySymbols`, `modifierSymbols`,
`separatorSymbols`) and their doc comments are removed (79 lines). What remains is the
`OperatorSymbol` enum, the precedence enum and lookup, and the rendering `String()`
method — the parts of the file that are about symbol identity rather than spelling
classification. **Why this site:** it is the shared home the remaining changes abandon;
**role:** symbol definitions still needed by every stage. This removal is what converts
"tables live in one place" into "each stage owns its own".

### `parsing.go` `readToken` — five lexer-local spelling tables

`readToken` gains `prefixLiterals` (`-`, `!`, `~`), `modifierLiterals` (11 spellings),
`logicalLiterals` (`&&`, `||`), `comparatorLiterals` (8 spellings), and `ternaryLiterals`
(`?`, `:`, `??`), declared once per call and used in exactly the lookup chain the old
registry served. **Why this site and shape:** `readToken` is the hottest and most-edited
function in the pipeline; function-local tables are the natural self-containment move
there and re-create the existing lookup sequence verbatim. **Divergence by design:** the
comparator copy intentionally omits the textual `in` spelling, because on this revision
`in`/`IN` is recognized in `readToken`'s variable branch (a keyword, not a symbol path) —
a real subtlety any later unification must reconcile rather than paper over. **Role:**
operator classification at tokenization time, including the prefix-versus-modifier
decision for `-`.

### `parsing.go` `optimizeTokens` — a two-entry regex comparator subset

The folding pass reads the full comparator table today and then immediately checks
whether the symbol is `REQ`/`NREQ`; the change gives it a local `regexComparators` table
with exactly `=~` and `!~`, and a `found` flag driven by membership in that table, so the
pass consults only the subset it actually needs. **Why this site:** it is the second
consumer in the same file, making the point that the relation is per-stage, not
per-file; **shape:** a two-entry membership probe is the minimal local need and reads as
a deliberate narrowing rather than as a copy of the full grammar. **Role:** decides which
comparators precompile their right-hand operand.

### `stagePlanner.go` `init` — seven phase-local acceptance tables

The planner's `init` gains `prefixPhaseLiterals`, `multiplicativePhaseLiterals`,
`additivePhaseLiterals`, `shiftPhaseLiterals`, `bitwisePhaseLiterals`,
`comparatorPhaseLiterals` (9 spellings including `in`), and `ternaryPhaseLiterals`, each
feeding its phase's `precedencePlanner.validSymbols`. The exponent phase keeps a
single-entry inline literal, identical to the pinned revision's `&&`/`||`/`,` inline
literals; `&&` and `||` retain their pre-existing inline single-entry form. **Why this
site:** precedence phases are the repository's own natural grouping of the grammar, so
one table per phase is the shape a maintainer of this file would actually write.
**Divergence by design:** the comparator phase's copy carries `in`, unlike the lexer's —
the planner consumes already-lexed comparator tokens, where `in` is just another
comparator. **Role:** which spellings each precedence level accepts when building the
evaluation-stage tree.

### `EvaluableExpression_sql.go` `findNextSQLString` — five dialect-local spelling tables

The SQL translator gains `logicalSqlLiterals`, `comparatorSqlLiterals` (9 spellings
including `in`), `ternarySqlLiterals`, `prefixSqlLiterals`, and `modifierSqlLiterals`,
and its five kind-dispatch `switch`es read the tables instead of the shared registry.
**Why this site:** translation is the stage furthest from lexing — it re-derives
spelling-to-symbol classification purely to choose SQL renderings, which is exactly the
coupling that makes "adjust the operator set" a multi-stop journey. **Shape:** mirror of
the lexer's set with the translator's own 9-spelling comparator copy (`in` is
translatable to SQL, so it must be classified here). **Role:** spelling dispatch during
SQL rendering.

## Deliberate structural variation

The table population varies along three axes so the spread cannot be reduced to one
repeated edit: scope (function-local in the lexer and translator vs phase-local in the
planner), subset (8- vs 9-spelling comparator copies, 2-entry regex subset, per-phase
subsets vs complete category sets), and coverage (the optimizer keeps no prefix or
ternary table at all; the planner keeps no modifier table beyond its phase sets). The
one-entry literals stay inline exactly as the pinned revision already did.
