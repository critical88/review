# Injection design record — `deeply_inlined_method` in coc-pyright

## Maintenance motivation

coc-pyright is a long-lived vim/Neovim extension that shells out to many external
Python tools (formatters, linters, sorters, test runners) and wraps a language
server. Its code has a recurring historical pattern: once a feature is patched in
a hurry, whole helper chains get copied *into* the outermost feature function so
a contributor can see "everything in one place" while debugging. The classic
trigger is a hard-to-reproduce, environment-dependent bug — a formatter that
works locally but not in CI, a linter that misbehaves only on stdin, a
test-runner that picks the wrong framework — because the developer wants the
exact bytes of every intermediate value while stepping through. Reinstrumenting
the shared helpers is unappealing (the shared base classes serve nine sibling
providers; adding debug taps there risks regressing everyone), so the pragmatic
shortcut is to inline the chain into one function, tweak it until the bug is
found, and never get around to pushing the code back out. This case models the
end state of that evolution after it has been rationalized into "deliberate"
local idioms: intermediate variables that suggest a different decomposition,
naming and structure that no longer mirror the helpers, and helper deletion in
the spots where the last external caller really had disappeared.

## Development evolution modeled

The realistic sequence being reproduced, per site:

1. A bug report arrives against a feature whose pipeline spans several files
   (`feature -> helper class/service -> second helper -> utility`).
2. The maintainer inlines the three-plus-level chain into the feature function,
   one level at a time, innermost first — the copied code initially reads
   verbatim with imports adjusted.
3. Debugging reshapes the copy: guard clauses become early returns, promise
   chains become try/catch/finally, closures become inline state machines,
   constants get local names, `else` branches disappear.
4. The copy gets a new comment vocabulary that presents the fused steps as the
   intended design ("replay the patch", "resolve the sorter", "drive the
   terminal"), i.e. a false decomposition.
5. Helpers whose only callers were on this chain are deleted from the owning
   file; helpers that still serve other features stay behind, leaving the
   inlined body as a *duplicate* of live code.
6. The repo-level static gate still passes (formatting, import style, and
   module-scoped linting rules are content-agnostic), the build is clean, and
   the change ships.

## Overall design

Six feature implementations across five responsibility areas are injected over
six production files. Each site is a function that absorbed a chain of calls
that previously crossed at least three levels and at least two other files, and
the absorbed copy is then reshaped into local idioms so no inlined fragment is
textually identical to its origin. Two distinct situations are represented:

- **Helper deletion sites.** Where the absorbed methods had no remaining
  callers, the originals are removed from the owning file, so the reader must
  reconstruct what the fused blob is doing and where its parts belong.
- **Helper duplication sites.** Where the absorbed methods are shared (the
  linter base classes, the diff-decoding utility, the python execution
  service), the originals remain for their many other users and the injected
  function carries a private, divergent copy. This mirrors the real evolution —
  the surrounding features never noticed — and it means the fused code is
  not simply "missing" but coexists with its own origin.

The absorbed chains cover four categories of external logic so the resulting
comprehension and repair burden is varied: process/execution plumbing, tool and
argument resolution, surface-syntax decoding (unified diff), and structured
result mapping (parse trees, LSP responses, regex-decoded tool output).

## Per-site design

### 1. Test-runner single-test command (`src/commands.ts`)

*Chain inlined:* command → parser-module parse → framework-specific test
walker → pyright-internal parse-tree access; plus command → shared test-runner
dial-up (module existence probe, interpreter version probe, terminal handling).

**What changed.** The single-test command handler now does its own parsing
(constructing the parse options, diagnostic sink, and parser directly), its own
recursive descent over the parse tree instead of the framework walker class to
enumerate test functions, its own suite-name reconstruction by walking parent
nodes, its own interpreter probes (a version probe for the importlib
machinery check, a module-existence probe), and its own terminal session
lifecycle (closing the previous buffer, disposing, recreating, and composing
the invocation line). The module-bound import of the parser module is gone;
node-child traversal is now driven directly through a node-utility function of
the vendored parser library.

**Why this site and shape.** Test-runner plumbing is famously the "works on my
machine" surface; the framework cannot be probed from shared helpers without
touching other commands, so inlining is the believable shortcut. The recursive
replacement for the walker was chosen because a copied walker class invites
textual comparison, whereas a local arrow-function descent with iteration and
guards looks like an intentional simplification. The whole-file variant command
keeps using the shared dial-up function, so the module now has both the shared
implementation and the fused copy side by side — exactly the duplication a
hurried patch leaves behind.

**Production role.** Selects the test function under the cursor and drives the
configured framework for it in a terminal.

### 2. Shared formatter pipeline (`src/features/formatters/baseFormatter.ts`)

*Chain inlined:* pipeline method → temp-file staging → execution-info
resolution → process execution → diff-decode utility → patch interpreter;
plus private cancellation, error-report, and teardown helpers.

**What changed.** The base pipeline method inherited by every formatter now
performs each of those steps in its own body: it resolves the executable and
module name inline, drives the execution service inline, decodes the
formatter's unified-diff output itself using a block/operation representation
(`originLine`, per-block op arrays) with a hand-written head-line character
cursor and a three-way pending-text state machine — deliberately different
vocabulary and structure from the shared utility's hunk-based replay, though
operationally equivalent. The private helpers for execution info, error
reporting, temp-file deletion, and cancellation checks are deleted (their last
callers were on this chain). The promise-chain control flow was converted to
async/await with try/catch/finally.

**Why this site and shape.** This is the highest-leverage site in the case:
every formatter id funnels through it, so a maintainer with a
formatter-specific misbehavior has the strongest motive to fuse it, and the
resulting method is the largest single body produced. The rewritten diff
decoder is the era's "better" local implementation — it intentionally does not
read like the shared utility, so recognizing that the two are the same
algorithm is genuine comprehension work. The polymorphic temp-file seam (one
subclass overrides how the staged file is created) was deliberately preserved
*as a call*, because a maintainer inlining for debugging observes that
override working and would not flatten it.

**Production role.** Formats the current document with the selected external
formatter and returns workspace edits.

### 3. Import-sorting command (`src/features/sortImports.ts`)

*Chain inlined:* command → provider-info builder (executable resolution,
per-provider arguments) → python execution service → process-service exec
options/spawn/decode → shared diff-decode utility; plus temp-file staging beside
the document.

**What changed.** The import-sorting command performs its own temp-file naming
(uri → path, hash sidecar name), its own provider resolution with inline
argument arrays for both supported sorters, an inline copy of the python
execution service's spawn policy (environment augmentation, buffering
environment entry, binary stderr handling, exit handling), drainage of stdout
and stderr via raw process events, and its own unified-diff replay driving
text edits for the document, including an inline declared-type for the diff
blocks and an inline flush procedure. The local provider-info and
diff-generation helpers had no other callers and are deleted.

**Why this site and shape.** Import sorting shares its head and tail with the
formatter pipeline but goes through a *different* service and a differently
shaped arguments list, so inlining it demonstrates the same smell relation over
a distinct chain; its temp-file policy (create before provider resolution,
delete only around the execution step with a leak-on-provider-error corner) is
subtle and must survive the fusion. This is the deepest absorbed chain in the
case (five concern levels inside one command function).

**Production role.** Sorts the imports of the current document using isort or
ruff and applies the result as edits.

### 4. Inlay-hints provider (`src/features/inlayHints.ts`)

*Chain inlined:* provider method → configuration filters → hover/signature LSP
request races → per-kind label conversion; plus a position-in-range utility.

**What changed.** The provider method decides inline which hint kinds the
configuration enables, computes inline whether a hint position falls in the
requested range (a local lexicographic guard instead of the shared range
comparator), races the language server for hover and signature help against
timeouts inline, and converts the three response kinds to label text inline via
restructured nested conditions. All six private helpers (two LSP races, three
label conversions, one configuration probe) plus the range-comparison helper
usage are gone; the range helper is removed from the imports because its last
caller was here.

**Why this site and shape.** This is the "many tiny helpers" shape of the
smell: nothing here is algorithmically deep, but the reader loses the named
intermediate concepts (what makes a variable hint, how a parameter label is
sliced) inside one provider loop. It was chosen to counterpoint the
process-plumbing sites with an LSP-interaction site, and the inlined localizations
are easy to check for behavior drift, making it a good mid-difficulty cluster
for a solver to confirm their diagnosis first.

**Production role.** Produces inlay type hints (variables, return types,
parameters) for a visible range.

### 5. Flake8 provider (`src/features/linters/flake8.ts`)

*Chain inlined:* provider run → shared linter base (execution, invocation
choice, output decoding, line regex splitting, severity mapping) → per-provider
info object (path resolution, default/config arguments, stdin support).

**What changed.** The flake8 provider's run method no longer delegates to its
base class pipeline: it resolves its own executable path from the configured
setting, appends its own argument list (format string honoring the shared
column offset, zero-exit flag), chooses stdin versus file invocation, drains
and decodes output itself via the named-capture line protocol, maps error types
to severities through a local closure (including the unknown-type fallback),
and corrects zero-or-negative columns itself. The module now holds a local
version of the line-matching interface and a local severity mapper. The shared
base pipeline and info construction remain in the codebase because the other
nine providers still use them.

**Why this site and shape.** Linter providers are the classic
"one provider misbehaves" case (flake8's column reporting and exit codes are
idiosyncratic), and the shared base's `parseMessages` being overridable means a
copy is plausible. The site demonstrates absorption where the origin *stays
alive*, so a solver cannot simply assume deleted helpers are the only story —
this inlined body is a divergent duplicate, and faithful repair must restore
delegation rather than "fixing" the shared base. Dynamic per-tool settings
reads (required: the subclass must consult settings whose keys carry the
provider id) are replaced by literal era-specific keys in the copy — a
faithful snapshot of a rushed inline-and-edit.

**Production role.** Runs flake8 over a document and decodes its findings into
provider messages.

### 6. Document linting engine (`src/features/linters/lintingEngine.ts`)

*Chain inlined:* pipeline method → document eligibility check → provider
factory construction → message-to-diagnostic conversion.

**What changed.** The document pipeline method now composes its eligibility
guards inline (settings toggle, language identity checked against a
language-constant list, ignore-pattern matching against configured patterns,
file existence) as early returns; constructs the provider for the product being
linted via an inline type switch instead of the factory method; and converts
provider messages to editor diagnostics inline, including the quote-based
column repositioning heuristic. The factory, eligibility, and conversion
methods had no remaining callers and are deleted.

**Why this site and shape.** The engine is where per-document policy and
per-product wiring meet, so it naturally reads as "just an orchestration
method" — the ideal place for a fused blob to hide in plain sight. The
eligibility guards become straight-line early returns (an intentionally
different idiom from the original boolean combination), and the conversion
loop with its two-level position correction is folded in so that the
diagnostic-shaping knowledge lives at the call site. The engine remains the
owner of the linter registry (construction still consults it for the active
linter set), so deletion of the local fused steps does not orphan the class.

**Production role.** Runs configured linters for a document, filtering
eligibility first, and turns their messages into editor diagnostics.

## Cross-site observations an auditor may want to check

- The six sites span four kinds of absorbed logic (process plumbing, tool
  resolution, diff decoding, structured-result mapping) and both duplication
  and deletion situations.
- The two diff-decode inlines use systematically different vocabulary and
  control idioms from the shared utility and from each other.
- Control-flow reshaping is applied everywhere (promise chains to
  try/catch/finally, walker classes to manual descent, boolean compositions to
  guard sequences, factory methods to inline switches), so no absorbed fragment
  is textually identical to its origin.
- The absorbed methods' other callers, sibling provider implementations, and the
  shared seams they still need (the override-capable temp-file step, the
  installation-error classifier, the vendored parser utilities) are untouched
  by construction, which is what keeps the change type-clean and behavior-flat
  over the whole verification suite.
