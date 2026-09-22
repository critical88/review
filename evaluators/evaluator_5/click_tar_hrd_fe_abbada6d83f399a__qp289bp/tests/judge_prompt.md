You are an expert code reviewer evaluating a refactored version of code that originally contained a "feature_envy" code smell.

## Context
- **Smell Type**: feature_envy
- **Smell Description**: A function that is more interested in data from other classes than its own, indicating misplaced behavior.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Change Analysis

### 1. Addition of `ParseResultProcessor` class in `_utils.py`

**What it does**: Introduces a new class that provides three methods: `normalize_params()` (replaces UNSET sentinels with None), `check_residual_args()` (validates extra arguments), and `apply_context_updates()` (merges parser prefixes with context prefixes).

**Significance**: **Critical** - This is the primary manifestation of the feature envy smell. The class exists solely to manipulate data from the `Context` object and parser state, yet it has no meaningful state of its own (the `_sentinel` and `_replacement` attributes are essentially constants).

**What it degrades**: 
- **Cohesion**: The class groups together unrelated operations that only share the property of being called after parsing. These operations don't form a cohesive concept.
- **Coupling**: Creates unnecessary coupling between `_utils.py` and parsing/context concepts.
- **Module responsibility**: Pollutes `_utils.py` (a utility module) with domain-specific parsing logic.
- **API surface**: Adds a public class that provides no real abstraction value.

### 2. Addition of `_ParseResultAdapter` class in `parser.py`

**What it does**: Creates an adapter that wraps parser results and delegates post-processing to `ParseResultProcessor`. The `finalize_context()` method orchestrates calling the processor's three methods and updating the Context object.

**Significance**: **Critical** - This is the wrapper that exhibits the feature envy behavior. The entire `finalize_context()` method reads and writes Context fields (`ctx.params`, `ctx.allow_extra_args`, `ctx.resilient_parsing`, `ctx.args`, `ctx._opt_prefixes`) without any meaningful transformation or local computation.

**What it degrades**:
- **Encapsulation**: Directly manipulates Context internals from outside, including the private `_opt_prefixes` field.
- **Cohesion**: The adapter has no meaningful identity beyond being a messenger between parser output and Context state.
- **Indirection**: Adds two layers (adapter + processor) where direct manipulation was clearer.
- **Law of Demeter**: Violates this principle by reaching through multiple objects to manipulate state.

### 3. Import of `_ParseResultAdapter` in `core.py`

**What it does**: Adds an import statement to bring the new adapter class into the Command module.

**Significance**: **Moderate** - This dependency import is necessary for the refactoring but reveals an architectural problem: the parser module now has logic that should belong to the Command/Context layer.

**What it degrades**:
- **Module dependencies**: Creates a circular conceptual dependency where parser now contains Context-manipulation logic, but Context uses parser.
- **Architecture clarity**: Blurs the boundary between parsing (tokenization/option extraction) and command execution (context management).

### 4. Replacement of inline logic in `Command.parse_args()` with adapter call

**What it does**: Removes 20 lines of straightforward inline code (UNSET normalization, extra args validation, context updates) and replaces it with a single adapter instantiation and method call.

**Significance**: **Critical** - This is where the original, well-located code is removed. The original code was appropriately placed: it had direct access to the context it was manipulating, and the operations were simple and clear.

**What it degrades**:
- **Readability**: The original code was explicit and easy to follow. The new code hides the logic behind two layers of indirection.
- **Locality**: The logic that operates on Context is moved away from where Context is naturally accessed.
- **Simplicity**: Replaces simple loops and conditionals with class instantiation and method delegation.

### 5. Static method `_format_extra_args_message()` in `ParseResultProcessor`

**What it does**: Extracts the `ngettext` call for formatting the "unexpected extra argument(s)" error message into a static method.

**Significance**: **Minor** - While this is technically unnecessary extraction, it's relatively harmless. However, it does contribute to the overall pattern of over-engineering.

**What it degrades**:
- **Simplicity**: Turns a simple inline format call into a method call.
- **Code ownership**: Moves message formatting away from the context where it's used (error reporting).

### 6. Comment changes in `Command.parse_args()`

**What it does**: Replaces clear implementation comments with abstraction-level comments about "delegation" and "cohesive steps."

**Significance**: **Minor** - The new comments try to justify the architectural choice rather than explain what's happening.

**What it degrades**:
- **Documentation clarity**: The original comments explained the "why" (handling UNSET sentinels). The new comments use buzzwords to justify indirection.

## Overall Smell Pattern

This diff demonstrates **feature envy** through unnecessary abstraction. The core violation is this: code that naturally belongs inside the `Command` class (or as helper methods on `Context`) has been extracted into two separate classes (`ParseResultProcessor` and `_ParseResultAdapter`) that have no meaningful state or behavior of their own. These classes exist solely to manipulate the data of the `Context` object.

The design principle violated is **Information Expert** (GRASP): the responsibility for manipulating an object's state should belong to that object or its natural collaborators. Here, `Context` contains all the data being manipulated (`params`, `args`, `allow_extra_args`, `resilient_parsing`, `_opt_prefixes`), but the manipulation logic has been moved to distant classes that must be given access to all these fields.

The smell is compounded by:
- **Inappropriate intimacy**: The adapter directly accesses private fields (`ctx._opt_prefixes`)
- **Middle man**: Both new classes act as unnecessary intermediaries
- **Speculative generality**: The "strategy object" and "adapter" patterns are applied without actual need for variation or adaptation

## Severity Ranking (Most to Least Important)

1. **Addition of `_ParseResultAdapter.finalize_context()` method** - This is the root cause. The method is pure feature envy: it exists only to read from and write to Context fields.

2. **Addition of `ParseResultProcessor` class** - While also problematic, this class is invoked by the adapter, making it a supporting player. However, it's still critical because it represents poor responsibility allocation.

3. **Replacement of inline logic in `Command.parse_args()`** - This change removes the original, well-placed code. It's critical because it shows what was lost.

4. **Import of `_ParseResultAdapter` in `core.py`** - Moderate importance as it reveals the architectural confusion but is technically just a dependency declaration.

5. **Static method `_format_extra_args_message()`** - Minor over-engineering that contributes to complexity but isn't core to the smell.

6. **Comment changes** - Cosmetic changes that reflect the misguided refactoring mindset.

## What Was Degraded Overall

**Coupling and Cohesion**: The changes created tight coupling between `_utils.py`, `parser.py`, and `core.py` while reducing cohesion within each module. The logic that was cohesively located in `Command.parse_args()` is now scattered across three files.

**Encapsulation**: The `Context` object's internals are now manipulated from external classes that have no business accessing private fields like `_opt_prefixes`.

**Simplicity and Readability**: The original code was straightforward: normalize params, check args, update context. The new code requires readers to trace through two class instantiations and three method calls across three files to understand the same operations.

**Maintainability**: Future developers must now understand an unnecessary abstraction layer. Changes to post-parse logic require modifications across multiple classes instead of a single, localized section.

**Testability**: While the new classes are theoretically more "testable," the original inline code was already perfectly testable through `Command.parse_args()` tests. The abstraction adds testing surface area without real benefit.

**Architectural Clarity**: The boundary between parsing (extracting tokens) and context management (tracking execution state) has been blurred. The parser module now contains context-manipulation logic.

## Key Evaluation Signals

A proper fix should be evaluated on:

1. **Data locality**: Does the code that manipulates Context data live where Context is naturally available and understood? The fix should eliminate remote manipulation of Context internals.

2. **Elimination of intermediary classes**: A thorough fix removes both `ParseResultProcessor` and `_ParseResultAdapter` entirely, recognizing they provide no real abstraction value.

3. **Restoration of directness**: The fix should return to direct manipulation of Context state from within `Command.parse_args()` or, at most, delegate to methods on the Context class itself.

4. **Module responsibility alignment**: `_utils.py` should contain true utilities (type hints, constants, simple helpers), not domain-specific parsing logic. The fix should remove domain logic from utility modules.

5. **Encapsulation preservation**: The fix should never expose or manipulate private fields (like `_opt_prefixes`) from external classes. If such access is needed, it should be through proper Context methods.

6. **Code volume and complexity**: A superficial fix might keep the abstraction layers but rename them. A proper fix recognizes that **less code is better** when the abstraction provides no value. The original inline code was simpler and should be restored.

7. **Comment quality**: After the fix, comments should explain "why" (e.g., why UNSET must be normalized), not justify architectural choices with buzzwords like "cohesive" and "strategy."

The key distinction: **A superficial fix might reorganize the smell (e.g., moving methods to Context), while a thorough fix eliminates the unnecessary abstraction entirely and returns to straightforward inline code where it belongs.**

**IMPORTANT**: The refactoring below is provided in **unified diff format** (git diff output). Lines starting with `-` are removed, lines starting with `+` are added, and context lines are unchanged. Evaluate the *intent and quality of the changes*, not the completeness of the code shown — diffs only show changed regions, not the full files.

### {label} Refactoring (diff fixing the smell)
```diff
{refactored_code}
```

## Evaluation Rubric

### General Evaluation Dimensions (score each 0-10)

1. **Smell Elimination Completeness** (score 0-10)
   - 10: Smell fully eliminated; no residual smell code, unused imports, or orphaned helpers remain
   - 8: Smell substantially eliminated; only minor traces remain
   - 6: Core smell addressed but some related artifacts (unused helpers, stale registrations) left behind
   - 4: Only the most obvious smell location fixed; secondary artifacts untouched
   - 2: Minimal effort; smell barely addressed
   - 0: Smell not addressed or new smell introduced
2. **Cross-File Coordination** (score 0-10)
   - 10: All smell-related code properly addressed; no orphaned imports, dead helpers, or dangling references left behind
   - 8: Nearly all cross-file impacts handled; one minor leftover
   - 6: Core changes correct; some related artifacts (unused helpers, stale imports) remain in other files
   - 4: Some cross-file changes made but several inconsistencies or leftovers
   - 2: Minimal cross-file awareness; related code in other files ignored
   - 0: Cross-file coordination largely missing or incorrect
3. **Structural Soundness** (score 0-10)
   - 10: Proper decomposition; single responsibility; appropriate abstraction
   - 8: Sound structure with minor imperfections
   - 6: Reasonable but some unnecessary complexity
   - 4: Noticeable structural issues; responsibilities not well separated
   - 2: Significant structural problems
   - 0: Introduces new code smells or anti-patterns
4. **Code Quality & Readability** (score 0-10)
   - 10: Clean, idiomatic, well-named, easy to maintain
   - 8: Good quality; minor naming or style improvements possible
   - 6: Acceptable quality; some naming or structural issues
   - 4: Below average; multiple readability concerns
   - 2: Poor quality; hard to follow
   - 0: Significantly worse readability than before

### Context for "feature_envy"

**Focus:** Whether the envious method is moved to the class whose data it primarily accesses, and whether data locality is improved.
{custom_rubrics_section}

### Difficulty Context
**Difficulty: Hard** (3-4 files). Expect handling of dynamic dispatch, red herrings, and design patterns. Focus on whether the agent correctly distinguishes real smells from intentional patterns.

## Output Format

Return your evaluation as JSON:
```json
{
  "smell_elimination": {
    "score": 0,
    "justification": "brief explanation"
  },
  "cross_file_coordination": {
    "score": 0,
    "justification": "brief explanation"
  },
  "structural_soundness": {
    "score": 0,
    "justification": "brief explanation"
  },
  "code_quality": {
    "score": 0,
    "justification": "brief explanation"
  },
  "summary": "2-3 sentence overall assessment"
}
```

**Important**: Return ONLY the JSON object, no other text before or after.
