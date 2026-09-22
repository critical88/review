You are an expert code reviewer evaluating a refactored version of code that originally contained a "god_classes" code smell.

## Context
- **Smell Type**: god_classes
- **Smell Description**: A class that centralizes too much functionality, violating single responsibility and becoming hard to maintain.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Changes Analysis

### 1. **Import statement added to `_eval_univariate` method in `ECDF` class**
```python
from .distributions import _DistributionPlotter
```
**What it does**: Creates a circular dependency by importing the distributions module into the statistics module at runtime (within a method).

**Significance**: **Critical**. This is a major architectural smell on its own. It creates tight coupling between two modules that should be independent. The statistics module now depends on the distributions module, while the distributions module was already depending on statistics (since it imports ECDF).

**What it degrades**: 
- Module independence and separation of concerns
- Testability (harder to test in isolation)
- Code comprehension (circular dependencies are notoriously difficult to reason about)
- Build/import order reliability

### 2. **Replacement of ECDF logic with delegation to `_DistributionPlotter`**
```python
return _DistributionPlotter._ecdf_eval_univariate_core(
    x, weights, self.stat, self.complementary,
)
```
**What it does**: The `ECDF` class now delegates its core computation to a static method in `_DistributionPlotter`, hollowing out its own implementation.

**Significance**: **Critical**. This inverts the natural dependency direction. A specialized statistical computation class (ECDF) should not depend on a plotting class. This violates the Dependency Inversion Principle and creates awkward coupling.

**What it degrades**:
- Class cohesion (ECDF no longer owns its logic)
- Architectural layering (statistics layer now depends on presentation layer)
- Reusability of ECDF outside the plotting context

### 3. **Addition of `_ecdf_init` method to `_DistributionPlotter`**
```python
def _ecdf_init(self, stat="proportion", complementary=False):
    """Store ECDF parameters on the plotter instance."""
    _check_argument("stat", ["count", "percent", "proportion"], stat)
    self._ecdf_stat = stat
    self._ecdf_complementary = complementary
```
**What it does**: Adds instance state management for ECDF parameters to the plotter class.

**Significance**: **Moderate to Critical**. This pollutes the plotter's state space with concerns that aren't inherently about plotting. The plotter now needs to track ECDF-specific configuration.

**What it degrades**:
- Single Responsibility Principle (plotter now manages statistical computation state)
- Class cohesion (mixing plotting state with computation parameters)
- API surface complexity (plotter has more state to manage)

### 4. **Addition of `_ecdf_eval_univariate_core` static method**
```python
@staticmethod
def _ecdf_eval_univariate_core(x, weights, stat, complementary):
    """Core univariate ECDF computation, usable as a static helper."""
    # [14 lines of computation logic copied from ECDF]
```
**What it does**: Duplicates the entire ECDF computation logic into the plotter class as a static method.

**Significance**: **Critical**. This is the core manifestation of the god class smell. The plotter class absorbs 14 lines of complex statistical computation logic that has nothing to do with plotting per se.

**What it degrades**:
- Single Responsibility (plotter now computes ECDFs)
- Class size and complexity (plotter grows)
- Code duplication risk (logic exists in two places conceptually)
- Maintainability (future ECDF changes must be made in the plotter)

### 5. **Addition of `_ecdf_eval_univariate` instance method**
```python
def _ecdf_eval_univariate(self, x, weights):
    """Evaluate the univariate ECDF using stored parameters."""
    return self._ecdf_eval_univariate_core(
        x, weights, self._ecdf_stat, self._ecdf_complementary,
    )
```
**Significance**: **Moderate**. This is a thin wrapper that uses the stored state to call the static method. It's less problematic than other changes but contributes to the bloat.

**What it degrades**:
- API surface (another method on an already complex class)
- Indirection (unnecessary delegation layer)

### 6. **Addition of `_ecdf_call` method**
```python
def _ecdf_call(self, x1, x2=None, weights=None):
    """Compute ECDF, dispatching univariate/bivariate."""
    # [9 lines including parameter validation and dispatch logic]
```
**Significance**: **Moderate to Critical**. This replicates the public API contract of the `ECDF.__call__` method inside the plotter, further absorbing ECDF's responsibilities.

**What it degrades**:
- Separation of concerns (plotter now has call semantics for ECDF)
- API clarity (plotter has ECDF-like behavior mixed in)

### 7. **Replacement of `ECDF` instantiation with `_ecdf_init` call**
```python
# OLD: estimator = ECDF(**estimate_kws)
# NEW: self._ecdf_init(**estimate_kws)
```
**Significance**: **Moderate**. This is the usage change that triggers all the above. Instead of creating a separate estimator object, the plotter now initializes itself with ECDF parameters.

**What it degrades**:
- Object-oriented design (composition replaced with state absorption)
- Clarity (less obvious that ECDF computation is happening)

### 8. **Multiple replacements of `estimator` references with `self._ecdf_*`**
```python
# OLD: stat, vals = estimator(observations, weights=weights)
# NEW: stat, vals = self._ecdf_call(observations, weights=weights)

# OLD: if estimator.stat == "count":
# NEW: if self._ecdf_stat == "count":

# OLD: stat = estimator.stat.capitalize()
# NEW: stat = self._ecdf_stat.capitalize()
```
**Significance**: **Minor individually, Moderate collectively**. These are the natural consequences of absorbing ECDF state into the plotter.

**What it degrades**:
- Code readability (loss of explicit estimator object makes the flow less clear)
- State management complexity (plotter's state space grows)

## Overall Smell Pattern

This diff creates a **god class** smell by violating the **Single Responsibility Principle** in multiple ways:

1. **Responsibility Absorption**: The `_DistributionPlotter` class absorbs the complete computational logic and state management of ECDF (Empirical Cumulative Distribution Function) computation, which should remain the responsibility of the `ECDF` class.

2. **Inverted Dependencies**: The change creates an architectural inversion where a lower-level utility (ECDF in statistics) depends on a higher-level consumer (the plotter in distributions), which then contains the actual implementation. This is backwards.

3. **State Pollution**: The plotter now maintains ECDF-specific state (`_ecdf_stat`, `_ecdf_complementary`) alongside its plotting state, bloating its state space.

4. **API Surface Explosion**: Four new methods are added to `_DistributionPlotter` (`_ecdf_init`, `_ecdf_eval_univariate_core`, `_ecdf_eval_univariate`, `_ecdf_call`), each handling concerns that aren't inherently about plotting.

The pattern here is **responsibility centralization**: instead of using composition (having the plotter use an ECDF object), the plotter becomes both a plotter AND an ECDF computer. This is the essence of a god class - accumulating multiple unrelated responsibilities.

## Severity Ranking (Most to Least Important)

1. **Addition of `_ecdf_eval_univariate_core` static method** - ROOT CAUSE. This 14-line computation is the actual wrongly-placed responsibility that makes the plotter a god class.

2. **Import statement in `_eval_univariate`** - ROOT CAUSE. Creates the circular dependency that enables the bad architecture.

3. **Replacement of ECDF logic with delegation** - ROOT CAUSE. Hollows out ECDF and inverts the dependency.

4. **Addition of `_ecdf_init` method** - MAJOR SYMPTOM. Pollutes plotter state with non-plotting concerns.

5. **Addition of `_ecdf_call` method** - MAJOR SYMPTOM. Replicates ECDF's public interface inside the plotter.

6. **Replacement of ECDF instantiation** - ENABLING CHANGE. Makes the bad design active in the codebase.

7. **Addition of `_ecdf_eval_univariate` instance method** - MINOR SYMPTOM. Thin wrapper, less problematic but contributes to bloat.

8. **Multiple `estimator` → `self._ecdf_*` replacements** - CONSEQUENCE. Natural follow-on changes from the core design problem.

## What Was Degraded Overall

**Architectural Quality:**
- **Module independence destroyed**: Circular dependency between statistics and distributions modules
- **Layering violated**: Statistics layer (lower) now depends on plotting layer (higher)
- **Dependency direction inverted**: ECDF depends on the plotter instead of vice versa

**Class Design Quality:**
- **Single Responsibility violated**: `_DistributionPlotter` now has two major responsibilities (plotting distributions AND computing ECDFs)
- **Cohesion decreased**: Plotter methods now span two unrelated concerns
- **Complexity increased**: 4 new methods, 2 new state variables, ~40 lines of code added to an already complex class
- **God class created**: Plotter centralizes too much functionality

**Maintainability:**
- **Harder to understand**: ECDF computation split across two modules in a confusing way
- **Harder to test**: Circular dependencies make isolated testing difficult
- **Harder to reuse**: ECDF functionality now requires pulling in the entire distributions module
- **Harder to modify**: Changes to ECDF logic now require touching the plotter class

**Code Organization:**
- **Duplication of concerns**: ECDF logic exists conceptually in both classes now
- **State management complexity**: Plotter's state space polluted with non-plotting parameters
- **API clarity reduced**: Less obvious that ECDF computation is a separate concern

## Key Evaluation Signals

When evaluating a fix for this smell, the most important criteria are:

### 1. **Dependency Direction Restoration (CRITICAL)**
- Does ECDF computation remain independent of the plotter?
- Is the circular import eliminated?
- Can the statistics module be used without importing distributions?
- Does the plotter depend on ECDF (correct) or vice versa (incorrect)?

### 2. **Responsibility Separation (CRITICAL)**
- Does `_DistributionPlotter` contain ECDF computation logic?
- Are ECDF parameters stored on the plotter instance?
- Can ECDF be used and tested independently of plotting context?
- Is the ECDF class self-contained with its own implementation?

### 3. **State Management Clarity (IMPORTANT)**
- Does the plotter have `_ecdf_stat` and `_ecdf_complementary` state variables?
- Is ECDF state managed through a separate object (composition) or absorbed into the plotter?

### 4. **API Surface (IMPORTANT)**
- Are the four ECDF-related methods removed from `_DistributionPlotter`?
- Is the plotter's public/internal API focused solely on plotting concerns?

### 5. **Usage Pattern (MODERATE)**
- Does `plot_univariate_ecdf` use composition (creating an ECDF object) or state mutation (calling `_ecdf_init`)?
- Is the relationship between plotter and ECDF clear at the usage site?

**Distinguishing thorough from superficial fixes:**
- **Superficial**: Renaming methods, adding comments, or extracting helper functions while keeping ECDF logic in the plotter
- **Thorough**: Completely removing ECDF computation and state from `_DistributionPlotter`, restoring ECDF as an independent, self-contained class, eliminating the circular dependency, and using composition in `plot_univariate_ecdf`

The fix MUST restore the original architecture where ECDF is independent and the plotter uses it, not absorbs it.

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
