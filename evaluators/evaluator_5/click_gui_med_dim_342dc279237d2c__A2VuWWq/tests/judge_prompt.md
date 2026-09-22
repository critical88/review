You are an expert code reviewer evaluating a refactored version of code that originally contained a "deeply_inlined_method" code smell.

## Context
- **Smell Type**: deeply_inlined_method
- **Smell Description**: A method whose sub-method implementations are copied into itself, creating extreme complexity. Depth 3 inlining makes the method exponentially hard to understand and refactor.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Change Analysis

### 1. New import: `_setup_main_invocation`
**What it does**: Adds an import for a new utility function that will be used to pre-process execution environment setup.

**Significance**: Minor in isolation, but enables the smell by providing a way to extract some logic while keeping the main complexity inline.

**What it degrades**: Slightly increases the API surface of the utils module, but this change itself is benign—it's what happens next that matters.

### 2. Introduction of `_setup_main_invocation()` function in utils.py
**What it does**: Extracts the logic for:
- Resolving the completion variable name from program name
- Reading the environment variable for shell completion instructions
- Merging `context_settings` with caller-provided `extra` kwargs

**Significance**: Moderate. This is the *only* extraction performed. It handles relatively straightforward setup logic (environment variable resolution and dictionary merging), leaving all the complex control flow inline.

**What it degrades**: Creates an **imbalanced** extraction pattern. The function extracts simple, easy-to-understand logic while abandoning the complex, error-prone control flow in the main method. This is a symptom rather than the cause—it shows that some refactoring was attempted but stopped far too early.

### 3. Replacement of `_main_shell_completion()` call with inline shell completion logic
**What it does**: Instead of delegating to a method, the code now:
- Checks `_cinstr` directly
- Imports `shell_complete` inline
- Calls it and exits

**Significance**: Minor to Moderate. This removes one level of indirection but the logic remains simple (a conditional check and early exit). The inline import is a minor code smell but not the main issue.

**What it degrades**: Slightly reduces modularity but the impact is limited since the logic is straightforward.

### 4. Replacement of `make_context()` with direct `Context` instantiation
**What it does**: Instead of using the `make_context()` helper method which presumably encapsulates context creation, the code now directly instantiates `self.context_class`.

**Significance**: **CRITICAL**. This is the beginning of the deeply inlined pattern. `make_context()` likely handled the context lifecycle in a clean, encapsulated way. By bypassing it, the code now must manually manage all context lifecycle details.

**What it degrades**: 
- **Cohesion**: Context creation logic is now split between what should be a single responsibility
- **Encapsulation**: Internal details of context management are exposed
- **Abstraction**: The abstraction provided by `make_context()` is violated

### 5. Manual context depth management and parse-phase scope handling (lines ~1421-1446)
**What it does**: Manually implements what was likely handled by `make_context()` and context managers:
- Increments `_ctx._depth` twice (once for parse phase, once for scope)
- Pushes context to thread-local stack
- Calls `parse_args()`
- Handles exceptions with `_pfail` flag
- Manually calls `_close_with_exception_info()` in finally block
- Pops context and decrements depth counters

**Significance**: **CRITICAL**. This is the heart of the "deeply inlined" smell. The code manually orchestrates context lifecycle management, resource cleanup, exception handling, and thread-local state management—all concerns that should be encapsulated behind abstractions.

**What it degrades**:
- **Readability**: The business logic (parsing arguments) is buried under infrastructure code
- **Maintainability**: The depth counter is manipulated 4 times in this block alone; any mistake in the increment/decrement symmetry will cause bugs
- **Error-proneness**: Manual exception handling with flags (`_pfail`) is fragile
- **Cognitive load**: Understanding this code requires tracking multiple state variables and control flow paths

### 6. Manual invoke-phase scope handling (lines ~1448-1489)
**What it does**: Similar manual orchestration for the invoke phase:
- Again increments depth and pushes context
- Invokes the command
- Captures exceptions in `_iexc` variable
- Manually handles context cleanup in finally block
- Conditionally suppresses exceptions based on context cleanup return value
- Re-raises if not suppressed
- Returns value if not in standalone mode

**Significance**: **CRITICAL**. This compounds the previous smell. The code now has TWO nearly-identical patterns of manual context lifecycle management, with subtle differences in exception handling.

**What it degrades**:
- **DRY principle**: The pattern of depth++, push, try/finally, cleanup, pop, depth-- is repeated
- **Complexity**: Nested control flow with multiple variables tracking exception state (`_iexc`, `_suppressed`, `_pfail`)
- **Testability**: Testing all branches of this control flow requires complex setup
- **Understanding**: The comment "The context-manager protocol (__enter__/__exit__) is expanded inline" is a red flag—it explicitly states that a well-understood abstraction has been violated

### 7. Variable naming with underscores
**What it does**: Uses underscore-prefixed names for local variables: `_cvar`, `_cinstr`, `_ctx_kw`, `_ctx`, `_pfail`, `_iexc`, `_rv`, `_suppressed`, etc.

**Significance**: Minor but notable. This convention is typically used for "private" or "temporary" variables, suggesting the author recognized these as implementation details.

**What it degrades**: Slightly reduces readability by making variable names less descriptive, but the underscore prefix does signal scope intent.

## Overall Smell Pattern

The "deeply_inlined_method" smell is created by **manually expanding abstractions that encapsulate complex control flow**. Specifically:

1. **Abstraction violation**: The code bypasses `make_context()` and manually implements context lifecycle management
2. **Context manager expansion**: The code explicitly inlines the `__enter__`/`__exit__` protocol instead of using `with` statements
3. **Imbalanced extraction**: Only trivial setup logic is extracted to `_setup_main_invocation()`, while complex orchestration remains inline

**Design principles violated**:
- **Separation of Concerns**: Business logic (parsing, invoking) is tangled with infrastructure (context management, cleanup)
- **Don't Repeat Yourself**: The depth/push/cleanup/pop pattern appears twice with variations
- **Favor Composition**: Instead of composing with context managers and helper methods, everything is flattened
- **Information Hiding**: Internal details (`_depth`, `_close_with_exception_info`) are exposed in the main control flow

The smell represents a **failed or incomplete refactoring** where abstractions were removed but not replaced with better ones.

## Severity Ranking (Most to Least Important)

1. **Manual parse-phase scope handling** (Change #5) - This introduces the pattern of manual lifecycle management
2. **Manual invoke-phase scope handling** (Change #6) - This duplicates and extends the pattern
3. **Replacement of make_context()** (Change #4) - This is the decision that enables #5 and #6
4. **Introduction of _setup_main_invocation()** (Change #2) - Shows imbalanced extraction priorities
5. **Replacement of _main_shell_completion()** (Change #3) - Minor reduction in modularity
6. **New import** (Change #1) - Enabler but not significant itself
7. **Variable naming** (Change #7) - Cosmetic issue only

**Root causes**: Changes #4, #5, and #6 together form the root cause. The decision to bypass `make_context()` and manually implement context lifecycle management is the fundamental problem.

## What Was Degraded Overall

**Concrete impacts:**

1. **Maintainability**: The method grew from presumably ~20 lines to 90+ lines of dense control flow. Any change to context lifecycle behavior now requires modifying this method instead of a focused helper.

2. **Coupling**: The main() method is now tightly coupled to:
   - Context's internal `_depth` attribute
   - Context's internal `_close_with_exception_info()` method
   - The thread-local context stack (push_context/pop_context)
   - Exception handling details that should be encapsulated

3. **Cohesion**: The method now mixes multiple responsibilities:
   - Setup (delegation to helper)
   - Shell completion (inline check)
   - Context lifecycle (manual management)
   - Parsing (business logic)
   - Invocation (business logic)
   - Exception handling (infrastructure)
   - Cleanup (infrastructure)

4. **Testability**: Testing this method now requires:
   - Mocking depth counters
   - Simulating various exception scenarios
   - Verifying correct cleanup order
   - Testing both standalone and non-standalone modes
   The test matrix exploded from testing a few delegation calls to testing complex state machines.

5. **Cognitive Load**: A developer reading this code must:
   - Track 7+ local variables simultaneously
   - Understand context depth semantics
   - Follow nested try/finally blocks
   - Reason about exception suppression logic
   - Understand the interaction between parse and invoke phases

6. **Abstraction Level**: The method operates at inconsistent abstraction levels—from high-level "invoke the command" to low-level "increment this counter, check this flag."

## Key Evaluation Signals

**What should matter most when judging a fix:**

1. **Context lifecycle encapsulation**: Does the fix properly encapsulate context lifecycle management behind abstractions (context managers, helper methods)? The depth counter manipulation should not appear in main().

2. **Control flow clarity**: Can a reader understand the main flow (setup → parse → invoke → cleanup) without getting lost in exception handling details?

3. **Elimination of manual bookkeeping**: Are variables like `_pfail`, `_iexc`, `_suppressed` eliminated in favor of proper abstraction boundaries?

4. **Use of language idioms**: Does the fix use `with` statements instead of manual `__enter__`/`__exit__` calls?

5. **Method length**: Is the method reduced to a reasonable length (ideally <30 lines) by proper delegation?

6. **Balanced extraction**: If helper functions are introduced, do they handle complex logic rather than just simple setup?

**Distinguishing thorough from superficial fixes:**

- **Superficial**: Extracts the inline code into a new private method but keeps the same manual lifecycle management. This just moves the smell without addressing the root cause.

- **Thorough**: 
  - Restores the use of `make_context()` or equivalent abstraction
  - Uses context managers (`with` statements) for lifecycle management
  - Separates exception handling concerns from business logic
  - Results in main() reading as a clear sequence of high-level operations
  - Eliminates direct manipulation of internal context state (_depth, etc.)

The litmus test: After the fix, can a developer understand what main() does without understanding how context lifecycle management works internally? If yes, the fix is thorough. If they still need to trace depth counters and exception flags, it's superficial.

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

### Context for "deeply_inlined_method"

**Focus:** Whether inlined code fragments are correctly identified and extracted back into well-scoped methods at the right abstraction level.
{custom_rubrics_section}

### Difficulty Context
**Difficulty: Medium** (2-3 files). Expect handling of indirect delegation and wrapper patterns. Be more lenient on edge cases.

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
