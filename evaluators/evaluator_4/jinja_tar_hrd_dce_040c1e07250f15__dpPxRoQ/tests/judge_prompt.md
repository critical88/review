You are an expert code reviewer evaluating a refactored version of code that originally contained a "dead_code_elimination" code smell.

## Context
- **Smell Type**: dead_code_elimination
- **Smell Description**: Code that is never executed or used, increasing complexity and maintenance burden.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Change Analysis

### 1. Import of `_middleware_registry` in compiler.py and environment.py
**What it does**: Adds imports for a new middleware registry data structure.
**Significance**: Minor - This is a supporting change to enable the middleware infrastructure.
**What it degrades**: Increases coupling by creating dependencies on a new utility module feature that is never actually used effectively.

### 2. New `_resolve_filter_func()` method in CodeGenerator (compiler.py)
**What it does**: Adds a method that resolves filter/test callables and checks middleware registry when `filter_test.middleware_mode` policy is not "passive".
**Significance**: **CRITICAL** - This is dead code at its core. The method retrieves a function, potentially iterates through middleware, but then just returns the same function without doing anything with the middleware information.
**What it degrades**: 
- **Readability**: Adds complexity suggesting middleware dispatch happens at compile time, which is misleading
- **Maintainability**: Creates false expectations about what the compiler does
- **API surface**: Adds an internal method that provides no value

### 3. Refactored filter/test function lookup in `_filter_test_common()` (compiler.py)
**What it does**: Moves the `environment.filters.get()` and `environment.tests.get()` calls from inline to the new `_resolve_filter_func()` method.
**Significance**: Moderate - This refactoring appears to add middleware awareness but actually changes nothing functionally.
**What it degrades**: **Indirection without purpose** - The original straightforward lookup is replaced with a method call that loops through middleware but ultimately returns the same result.

### 4. New `_validate_policy_defaults()` function (defaults.py)
**What it does**: Validates that policy keys ending with "_mode" are strings.
**Significance**: Minor to Moderate - This validates configuration but only for the new (unused) middleware mode policy.
**What it degrades**: Adds defensive code for a feature that doesn't work, bloating the validation logic.

### 5. Addition of `filter_test.middleware_mode: "passive"` policy (defaults.py)
**What it does**: Adds a new policy defaulting to "passive" mode.
**Significance**: **CRITICAL** - This is the configuration hook for the entire dead middleware system.
**What it degrades**: **Configuration surface area** - Adds a policy that controls a non-functional feature, confusing users about what it does.

### 6. New `_prepare_call_args()` static method (environment.py)
**What it does**: Normalizes filter/test arguments into mutable lists and dicts.
**Significance**: Moderate - Actually used, but exists solely to support the middleware path.
**What it degrades**: Adds complexity to argument handling where simple inline conversion would suffice.

### 7. New `_invoke_with_middleware()` method (environment.py)
**What it does**: Implements the runtime middleware dispatch chain - calls `pre_invoke` on middleware, executes the function, calls `post_invoke` in reverse order.
**Significance**: **CRITICAL** - This is substantial, complex code that is unreachable because the policy defaults to "passive" and is never documented or exposed.
**What it degrades**:
- **Cohesion**: Adds complex orchestration logic to Environment class
- **Maintainability**: ~30 lines of branching logic that will never execute
- **Performance**: Even checking the policy mode adds overhead to every filter/test call

### 8. Refactored `_filter_test_common()` to check middleware mode (environment.py)
**What it does**: Adds conditional logic to dispatch through middleware when mode is not "passive".
**Significance**: **CRITICAL** - This is the unreachable branch point.
**What it degrades**: **Control flow complexity** - Every filter/test invocation now checks a policy that always has the same value.

### 9. `_FilterTestMiddleware` base class (utils.py)
**What it does**: Defines an abstract base for middleware with `should_intercept`, `pre_invoke`, and `post_invoke` methods.
**Significance**: **CRITICAL** - This is a complete class hierarchy for a feature that's never active.
**What it degrades**:
- **API surface**: Exposes a public-looking base class for an internal, non-functional feature
- **Conceptual load**: Developers must understand middleware concepts that don't apply

### 10. Enhanced `_PassArg.from_obj()` validation (utils.py)
**What it does**: Adds type checking to ensure the `jinja_pass_arg` attribute is actually a `_PassArg` enum value.
**Significance**: Minor - Defensive programming that's not directly related to dead code.
**What it degrades**: Minimal impact; slight complexity increase.

### 11. `_middleware_registry` global dictionary (utils.py)
**What it does**: Creates a global registry with "filter" and "test" categories for storing middleware instances.
**Significance**: **CRITICAL** - Global mutable state for a non-functional feature.
**What it degrades**:
- **Testability**: Global state is notoriously hard to test
- **Thread safety**: Mutable global without synchronization
- **Coupling**: Anything can mutate this registry

### 12. `register_middleware()` function (utils.py)
**What it does**: Public API for registering middleware instances, with priority-based sorting.
**Significance**: **CRITICAL** - Public API function for a feature that doesn't work.
**What it degrades**: **Public API surface** - Adds a function users might discover and try to use, only to find it does nothing by default.

### 13. `_FilterArgNormalizer` middleware class (filters.py)
**What it does**: Concrete middleware that strips None-valued kwargs for certain filters.
**Significance**: **CRITICAL** - Fully implemented, registered middleware that never executes.
**What it degrades**: 
- **Maintenance burden**: ~20 lines of code with logic that must be kept consistent with filter signatures
- **False documentation**: Code suggests this normalization happens, but it doesn't

### 14. Registration of `_FilterArgNormalizer` (filters.py)
**What it does**: Calls `register_middleware()` to add the normalizer at module import time.
**Significance**: Moderate - Side effect at import time for no benefit.
**What it degrades**: **Module import cost** - Adds work during import that has no effect.

### 15. `_TestResultCoercer` middleware class (tests.py)
**What it does**: Concrete middleware that coerces test results to bool for operator-based tests.
**Significance**: **CRITICAL** - Another fully implemented, never-executed middleware.
**What it degrades**: Same as `_FilterArgNormalizer` - maintenance burden and false expectations.

### 16. Registration of `_TestResultCoercer` (tests.py)
**What it does**: Registers the coercer middleware at import time.
**Significance**: Moderate - Import-time side effect with no impact.
**What it degrades**: Module import cost and conceptual complexity.

## Overall Smell Pattern

This is a **speculative generality** variant of dead code elimination. The diff introduces a complete **middleware infrastructure** for intercepting and transforming filter/test invocations, including:
- Policy configuration
- Registration system
- Abstract base class
- Two concrete implementations
- Compile-time and runtime dispatch paths

However, the entire system is **unreachable by design** because:
1. The policy defaults to "passive" mode
2. There's no documented way to change it
3. The middleware is never activated even if registered
4. The compile-time path checks middleware but does nothing with the information

**Design principles violated**:
- **YAGNI (You Aren't Gonna Need It)**: Building elaborate infrastructure before it's needed
- **Single Responsibility**: Environment class now handles middleware orchestration
- **Open/Closed**: The middleware system suggests extensibility but isn't actually usable
- **Principle of Least Surprise**: Code structure implies functionality that doesn't exist

## Severity Ranking (Most to Least Critical)

1. **`_invoke_with_middleware()` method** - Most complex dead code (~30 lines of orchestration logic)
2. **Middleware registration system** (`register_middleware`, `_middleware_registry`) - Public API for non-functional feature
3. **`filter_test.middleware_mode` policy** - Configuration that gates all the dead code
4. **Concrete middleware classes** (`_FilterArgNormalizer`, `_TestResultCoercer`) - Fully implemented but never executed
5. **`_FilterTestMiddleware` base class** - Complete abstraction for unused feature
6. **Runtime dispatch check in `_filter_test_common()`** - Branch that's never taken
7. **`_resolve_filter_func()` in compiler** - Misleading compile-time middleware check
8. **`_prepare_call_args()` helper** - Supporting method for dead code path
9. **`_validate_policy_defaults()`** - Validation for the non-functional policy
10. **Import statements and minor refactorings** - Supporting infrastructure

## What Was Degraded Overall

**Concrete impacts:**
1. **Code size**: ~150+ lines of dead code across 5 files
2. **Complexity**: Added cyclomatic complexity in hot paths (every filter/test call checks policy)
3. **API surface**: New public functions (`register_middleware`) and base classes that don't work
4. **Coupling**: Multiple modules now depend on `_middleware_registry` from utils
5. **Cognitive load**: Developers must understand middleware concepts that don't actually function
6. **Maintainability**: Future changes must consider this infrastructure even though it's inactive
7. **Performance**: Every filter/test call now has an extra policy lookup (even if minimal)
8. **Testability**: No tests can exercise this code because it's unreachable without changing defaults
9. **Documentation debt**: Code implies behavior that would need documentation, but doesn't work
10. **Import time cost**: Middleware registration happens at import but provides no value

## Key Evaluation Signals

To judge whether a fix truly addresses this smell:

### Most Important:
1. **Complete removal of middleware infrastructure** - Not just disabling, but removing the base class, registry, and registration function
2. **Elimination of runtime policy checks** - The `_filter_test_common()` conditional branch should be removed
3. **Removal of concrete middleware implementations** - Both `_FilterArgNormalizer` and `_TestResultCoercer` must go
4. **Restoration of simple function lookups** - The compiler should go back to inline `environment.filters.get()` calls

### Distinguishing thorough from superficial:
- **Thorough**: Removes all middleware-related code, policy, helpers, and imports. Restores original straightforward logic.
- **Partial**: Removes concrete middleware but leaves base class or registry "for future use"
- **Superficial**: Comments out or conditionally disables middleware but leaves code in place

### Warning signs of incomplete fixes:
- Leaving `_FilterTestMiddleware` base class "just in case"
- Keeping `register_middleware()` as "API for plugins"
- Retaining policy with comment like "reserved for future"
- Leaving `_invoke_with_middleware()` marked as deprecated

### Gold standard:
The fix should make it as if the middleware concept was never introduced. The code should return to the straightforward filter/test dispatch that existed before, with no vestiges of the middleware pattern remaining.

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

### Context for "dead_code_elimination"

**Focus:** Whether dead code is accurately identified and completely removed without leaving orphaned references.
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
