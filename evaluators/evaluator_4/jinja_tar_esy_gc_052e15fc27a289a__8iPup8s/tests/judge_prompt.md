You are an expert code reviewer evaluating a refactored version of code that originally contained a "god_classes" code smell.

## Context
- **Smell Type**: god_classes
- **Smell Description**: A class that centralizes too much functionality, violating single responsibility and becoming hard to maintain.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Change Analysis

### 1. Addition of `_owner_context` field to `EvalContext`
**What it does**: Adds an optional reference back from `EvalContext` to its owning `Context` object. This creates a bidirectional dependency where `EvalContext` previously had no knowledge of `Context`.

**Significance**: **Critical**. This is the foundational change that enables the god class pattern. By adding this back-reference, `EvalContext` becomes coupled to `Context`, allowing it to delegate operations back to its owner.

**What it degrades**: 
- **Coupling**: Creates tight bidirectional coupling between `EvalContext` and `Context`
- **Cohesion**: `EvalContext` loses its independence and becomes dependent on external state
- **Testability**: `EvalContext` can no longer be tested in isolation without a `Context` instance

### 2. Modification of `EvalContext.save()` to delegate to owner
**What it does**: Changes `save()` from a simple state-saving method to one that conditionally delegates to `_owner_context.save_eval_context()` if an owner exists.

**Significance**: **Critical**. This transforms `EvalContext` from a self-contained object to one that may outsource its core responsibility. The method's behavior now depends on external context, violating the Single Responsibility Principle.

**What it degrades**:
- **Predictability**: The method has two completely different behaviors based on runtime state
- **Encapsulation**: The object's state management is no longer self-contained
- **Method complexity**: Introduces conditional logic where none was needed

### 3. Modification of `EvalContext.revert()` to delegate to owner
**What it does**: Similar to `save()`, changes `revert()` to conditionally delegate to `_owner_context.revert_eval_context(old)`.

**Significance**: **Critical**. Completes the delegation pattern started in `save()`, fully outsourcing state management responsibility.

**What it degrades**: Same issues as `save()` - predictability, encapsulation, and adds conditional complexity.

### 4. Addition of `Context._make_eval_context()`
**What it does**: Introduces a factory method that creates an `EvalContext` and sets its `_owner_context` back-reference to `self`.

**Significance**: **Moderate**. This is a reasonable factory pattern in isolation, but it's used here to establish the problematic back-reference coupling.

**What it degrades**:
- **Initialization complexity**: What was a simple constructor call becomes a factory method
- **API surface**: Adds another method to the already complex `Context` class

### 5. Addition of `Context.save_eval_context()`
**What it does**: Adds a method to `Context` that simply calls `self.eval_ctx.__dict__.copy()`.

**Significance**: **Moderate to Critical**. This is a key symptom of the god class smell - `Context` is now responsible for operations that `EvalContext` should handle itself.

**What it degrades**:
- **Responsibility distribution**: `Context` takes on duties that belong to `EvalContext`
- **Encapsulation**: `Context` directly manipulates `EvalContext`'s internal `__dict__`
- **API bloat**: Adds to `Context`'s already large interface

### 6. Addition of `Context.revert_eval_context()`
**What it does**: Parallel to `save_eval_context()`, adds a method that manipulates `EvalContext`'s internal state.

**Significance**: **Moderate to Critical**. Another responsibility absorbed by `Context` that should remain with `EvalContext`.

**What it degrades**: Same as `save_eval_context()` - responsibility distribution, encapsulation, API bloat.

### 7. Addition of `Context.get_autoescape()`
**What it does**: Adds a getter method that returns `self.eval_ctx.autoescape`.

**Significance**: **Minor to Moderate**. This is a simple accessor, but it's symptomatic of `Context` becoming a façade for `EvalContext` operations.

**What it degrades**:
- **Directness**: Adds unnecessary indirection for a simple property access
- **API surface**: Yet another method on `Context`
- **Law of Demeter**: Encourages clients to go through `Context` instead of accessing `eval_ctx` directly

### 8. Changes to `BlockReference` to use `_context.get_autoescape()`
**What it does**: Updates two call sites to use the new getter method instead of directly accessing `_context.eval_ctx.autoescape`.

**Significance**: **Minor**. These are client-side changes that follow from the new API, but they don't fundamentally change architecture.

**What it degrades**:
- **Readability**: Slightly more verbose
- **Performance**: Adds an extra method call (negligible but unnecessary)

### 9. Change in `Context.__init__()` to use factory method
**What it does**: Replaces direct instantiation of `EvalContext` with a call to `_make_eval_context()`.

**Significance**: **Moderate**. This is where the coupling is established at runtime.

**What it degrades**:
- **Clarity**: The initialization is now indirect
- **Coupling establishment**: This is where the bidirectional relationship is created

## Overall Smell Pattern

This diff implements a **god class** pattern by centralizing `EvalContext`-related responsibilities within the `Context` class. The pattern works through three mechanisms:

1. **Back-reference coupling**: `EvalContext` is given a reference to `Context` (`_owner_context`)
2. **Responsibility delegation**: `EvalContext` methods delegate their core functionality to `Context` methods
3. **Responsibility absorption**: `Context` takes on methods that should belong to `EvalContext`

The fundamental design principle violated is the **Single Responsibility Principle**. `Context` now has responsibility for:
- Its own context management (original responsibility)
- Eval context state management (save/revert operations)
- Eval context property access (autoescape getter)

Additionally, this violates **proper encapsulation** - `EvalContext` objects that have an owner behave fundamentally differently from those that don't, creating two distinct object behaviors within the same class.

## Severity Ranking (Most to Least Important)

1. **Addition of `_owner_context` field** - Root cause that enables all other problems
2. **Modification of `save()` and `revert()` to delegate** - Core architectural violation where object outsources its responsibility
3. **Addition of `save_eval_context()` and `revert_eval_context()` to Context** - Where `Context` absorbs responsibilities
4. **Change to use `_make_eval_context()` in `__init__`** - Where the coupling is established
5. **Addition of `_make_eval_context()` factory** - Mechanism for coupling establishment
6. **Addition of `get_autoescape()`** - Minor symptom of façade pattern
7. **Changes to `BlockReference`** - Downstream effects, not core to the smell

## What Was Degraded Overall

**Coupling**: Transformed from clean unidirectional dependency (Context → EvalContext) to tight bidirectional coupling. `EvalContext` now cannot exist meaningfully without knowing about `Context`.

**Cohesion**: Both classes lost cohesion. `EvalContext` has split behavior (with/without owner), while `Context` has taken on unrelated eval context management duties.

**Single Responsibility**: `Context` now manages both its own state and its sub-object's state, becoming a "god" object that knows and controls too much.

**Testability**: `EvalContext` with an owner cannot be tested independently. Tests must now set up the full `Context` hierarchy even when only testing eval context behavior.

**Maintainability**: Changes to state management now require coordinating changes across both classes. The delegation pattern obscures the actual implementation location.

**API clarity**: `Context` API has grown with methods that aren't conceptually part of context management, making it harder to understand the class's purpose.

**Encapsulation**: Both classes violate encapsulation - `EvalContext` exposes its behavior to external control, and `Context` manipulates `EvalContext` internals directly via `__dict__`.

## Key Evaluation Signals

A thorough fix should:

1. **Eliminate bidirectional coupling**: The `_owner_context` back-reference should be removed. `EvalContext` should be independent of `Context`.

2. **Restore responsibility to EvalContext**: The `save()` and `revert()` methods should be fully self-contained within `EvalContext` without any delegation logic.

3. **Remove absorbed responsibilities from Context**: Methods like `save_eval_context()`, `revert_eval_context()`, and `get_autoescape()` should be removed from `Context`.

4. **Restore direct access patterns**: Client code like `BlockReference` should access `eval_ctx.autoescape` directly rather than going through `Context` getters.

5. **Simplify initialization**: `Context.__init__` should directly instantiate `EvalContext` without a factory method (unless the factory serves a legitimate purpose beyond establishing coupling).

**Distinguishing thorough from superficial fixes**:
- **Superficial**: Removing just the getter methods or renaming things without addressing the back-reference and delegation
- **Thorough**: Completely removing the bidirectional dependency, ensuring `EvalContext` is fully self-sufficient, and restoring clear responsibility boundaries

The key signal is whether `EvalContext` can function completely independently without any knowledge of or reference to `Context`. If it still needs `Context` to work properly, the god class smell persists.

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

### Context for "god_classes"

**Focus:** Whether distinct responsibilities are correctly identified and extracted into separate, cohesive classes.
{custom_rubrics_section}

### Difficulty Context
**Difficulty: Easy** (1-2 files). Expect straightforward refactoring. Penalize heavily for missed files.

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
