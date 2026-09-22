You are an expert code reviewer evaluating a refactored version of code that originally contained a "dead_code_elimination" code smell.

## Context
- **Smell Type**: dead_code_elimination
- **Smell Description**: Code that is never executed or used, increasing complexity and maintenance burden.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Change Analysis

### 1. `_backend_capabilities` and `_detect_backend_capabilities()` in `_statistics.py`

**What it does**: Introduces a global capability detection system that checks for the presence of `statsmodels` and its various components (GLM, RLM, WLS with iterative_fit). It caches the result in a global variable and returns a set of capability strings.

**Significance**: **Critical** - This is a core component of the dead code infrastructure. It creates an elaborate capability detection mechanism that is never actually needed.

**What it degrades**:
- **Module cohesion**: `_statistics.py` should contain statistical utilities, not backend detection logic
- **API surface**: Adds unnecessary global state and public functions
- **Complexity**: Introduces caching, set manipulation, and multiple capability strings that serve no real purpose
- **Testability**: Global mutable state makes testing harder

### 2. `check_capability()` in `_statistics.py`

**What it does**: A thin wrapper that checks if a capability name exists in the detected capabilities set.

**Significance**: **Critical** - This is the interface through which the dead code pretends to be useful. It's the only part that's actually called from existing code.

**What it degrades**:
- **Indirection**: Adds unnecessary abstraction layer
- **API pollution**: Expands the module's public interface with functionality that duplicates existing checks

### 3. Registration system in `algorithms.py` (`register_regression_backend`, `get_regression_backend`, `available_backends`, `_regression_backends`, `_backend_priorities`)

**What it does**: Creates a plugin-style registration system for regression backends with priority sorting and capability requirements. Includes global dictionaries to store registered backends and their metadata.

**Significance**: **Critical** - This is the most elaborate piece of dead code. It implements a full plugin architecture that is never actually used for its intended purpose.

**What it degrades**:
- **Module cohesion**: `algorithms.py` contains bootstrap utilities, not plugin registration systems
- **Over-engineering**: Implements a flexible plugin system where a simple function would suffice
- **Global state**: Two global dictionaries that complicate module initialization and testing
- **Maintenance burden**: Complex code that future maintainers must understand but provides no value

### 4. `_weighted_ols_fit()` in `algorithms.py`

**What it does**: Implements a weighted OLS regression with Huber-type weight estimation, including automatic weight calculation based on residuals.

**Significance**: **Moderate** - This is actual functional code with mathematical logic, but it's never effectively used.

**What it degrades**:
- **Clarity**: Adds domain-specific statistical code to a module focused on general algorithms
- **Documentation debt**: Non-trivial statistical method without proper mathematical documentation
- **Module scope creep**: `algorithms.py` now contains specific regression implementations

### 5. `register_regression_backend("weighted_ols", ...)` call in `algorithms.py`

**What it does**: Registers the weighted OLS implementation with high priority (10) and a requirement for "iterative_wls" capability.

**Significance**: **Critical** - This is where the dead code becomes obvious. The registration requires a capability ("iterative_wls") that can never be satisfied because `sm.WLS.iterative_fit` doesn't exist in statsmodels.

**What it degrades**:
- **Logic coherence**: Creates an unreachable code path by design
- **Module initialization**: Executes registration code at import time that serves no purpose

### 6. Modified `_check_statsmodels()` in `regression.py`

**What it does**: Replaces the direct check of `_has_statsmodels` with a call to `check_capability("statsmodels")`.

**Significance**: **Moderate** - This is the only place where the new code connects to existing functionality, but it's purely a replacement of existing working code.

**What it degrades**:
- **Coupling**: Introduces unnecessary dependency on `_statistics` module
- **Performance**: Adds function call overhead and capability detection for a simple boolean check
- **Clarity**: Obscures a simple presence check with abstraction

### 7. `_select_regression_backend()` in `utils.py`

**What it does**: Iterates through registered backends by priority and checks if their capability requirements are met, returning the first matching backend.

**Significance**: **Critical** - This is the selection logic that would choose which backend to use, but it's fundamentally broken because no backend can satisfy its requirements.

**What it degrades**:
- **Module cohesion**: `utils.py` shouldn't contain regression-specific backend selection logic
- **Complexity**: Adds iteration, capability checking, and conditional logic that always returns `(None, None)`
- **Function signature**: Takes unused parameters (`x`, `y`, `fit_kwargs`) suggesting unfinished implementation

### 8. Backend resolution code in `fit_regression()` in `regression.py`

**What it does**: Calls `_select_regression_backend()` to determine which backend to use, storing the result in `backend_name` and `backend_func`.

**Significance**: **Critical** - This is where the dead code integrates into the main execution path, but the variables it creates are never meaningfully used.

**What it degrades**:
- **Code clarity**: Introduces variables that appear important but do nothing
- **Execution overhead**: Adds function call at every fit operation
- **Misleading structure**: Suggests backend selection matters when it doesn't

### 9. New conditional branch in `fit_regression()` for weighted_ols

**What it does**: Adds an `elif` branch that would call `self.fit_weighted()` if `backend_name == "weighted_ols"` and `self.order == 1`.

**Significance**: **Critical** - This is the only place where the backend selection would matter, but this branch is unreachable.

**What it degrades**:
- **Control flow complexity**: Adds conditional branch that increases cyclomatic complexity without adding functionality
- **Dead code in execution path**: Creates unreachable code in a critical method

### 10. `fit_weighted()` method in `_RegressionPlotter` class

**What it does**: Implements a complete weighted regression fitting method with bootstrap support, properly integrated with the class's existing bootstrap infrastructure.

**Significance**: **Moderate** - This is well-implemented code that fits the class's pattern, but it's never called.

**What it degrades**:
- **Class complexity**: Adds ~20 lines to an already complex class
- **API surface**: Expands the class's method set with unused functionality
- **Maintenance burden**: Future maintainers must understand this method even though it's never used

## Overall Smell Pattern

This diff introduces a **elaborate infrastructure for dead code** - a multi-layered system spanning four modules that implements:
1. Capability detection (checking for features that don't exist)
2. Backend registration (registering backends with impossible requirements)
3. Backend selection (always returning None)
4. Conditional execution (unreachable branches)

The design principle violated is **YAGNI (You Aren't Gonna Need It)** and **Keep It Simple**. The code creates:
- An extensible plugin architecture where none is needed
- A capability detection system that checks for non-existent features
- A selection mechanism that can never succeed
- Integration points that will never execute

The smell is particularly insidious because:
- It looks professional and well-structured
- It follows reasonable patterns (plugin systems, capability detection)
- It integrates with existing code just enough to seem legitimate
- The dead code isn't immediately obvious without understanding that `sm.WLS.iterative_fit` doesn't exist

## Severity Ranking (Most to Least Important)

1. **Registration call with impossible requirement** - This is the smoking gun. Requiring "iterative_wls" that can never be detected makes the entire system dead on arrival.

2. **Backend selection logic in `fit_regression()`** - The integration point that executes on every fit but accomplishes nothing. This directly impacts runtime performance.

3. **`_select_regression_backend()` function** - The never-succeeding selection logic that creates the illusion of functionality.

4. **Registration system infrastructure** - The elaborate plugin system (`register_regression_backend`, global dictionaries, etc.) that supports the dead code.

5. **Capability detection system** - While unnecessary, at least `check_capability("statsmodels")` gets used once (even though it's redundant).

6. **`fit_weighted()` method** - Well-implemented but unreachable code. Less critical because it's isolated.

7. **`_weighted_ols_fit()` function** - The actual algorithm implementation. Least critical because it's just a helper function that could theoretically be useful if the system worked.

8. **Modified `_check_statsmodels()`** - Replaces working code with equivalent functionality, so minimal impact.

## What Was Degraded Overall

**Coupling**: 
- `regression.py` now depends on `_statistics.py`, `algorithms.py`, and `utils.py` for backend selection
- `_statistics.py` now imports and checks `statsmodels` in ways that duplicate existing checks
- Cross-module dependencies increased from simple imports to runtime function calls

**Cohesion**:
- `algorithms.py` mixed bootstrap utilities with backend registration
- `_statistics.py` mixed validation utilities with capability detection
- `utils.py` mixed general utilities with regression-specific selection logic

**Complexity**:
- Added ~140 lines of code across four modules
- Introduced global state in two modules
- Added multiple levels of indirection (capability → backend → selection → execution)
- Increased cyclomatic complexity in `fit_regression()`

**Maintainability**:
- Future maintainers must understand a non-functional system
- Testing surface expanded with untestable/unreachable code
- Documentation debt for undocumented statistical methods
- Debugging complexity increased (why isn't weighted fitting working?)

**Performance**:
- Every call to `fit_regression()` now performs backend selection that always fails
- Capability detection with set operations and imports

**API Surface**:
- Multiple new public functions that shouldn't be used
- Global state that could cause issues in concurrent contexts
- New method on `_RegressionPlotter` that's never called

## Key Evaluation Signals

A **thorough fix** must:

1. **Remove the impossible requirement**: The "iterative_wls" capability check that can never succeed is the root cause. Any fix that leaves this or similar impossible conditions is superficial.

2. **Remove cross-module registration infrastructure**: The entire `register_regression_backend()`, global dictionaries, and related functions should be eliminated. If these remain, the architectural smell persists even if specific instances are removed.

3. **Remove backend selection from execution path**: The call to `_select_regression_backend()` in `fit_regression()` must be removed, not just modified. If backend selection remains but with different logic, the smell persists.

4. **Clean up capability detection if unused**: If `check_capability()` is only used to replace `_has_statsmodels`, it should be removed and the original check restored. If it's completely unused, removal is mandatory.

5. **Remove unreachable conditional branch**: The `elif backend_name == "weighted_ols"` branch and `fit_weighted()` method must be removed entirely, not just commented out or modified.

6. **Restore module cohesion**: Functions should return to appropriate modules (regression logic out of `algorithms.py` and `utils.py`, capability detection out of places it doesn't belong).

**Distinguishing thorough from superficial**:
- **Superficial**: Removes only the registration call or only `fit_weighted()`, leaving infrastructure
- **Superficial**: Keeps backend selection but makes it "work" by fixing requirements
- **Superficial**: Comments out code instead of removing it
- **Thorough**: Removes entire dead code infrastructure across all four modules
- **Thorough**: Reduces cross-module coupling back to original levels
- **Thorough**: Eliminates all unreachable code paths and unused functions
- **Thorough**: Restores original simplicity (e.g., direct `_has_statsmodels` check)

The key test: After the fix, can you trace any remaining code from the diff that serves no purpose? If yes, the fix is incomplete.

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
