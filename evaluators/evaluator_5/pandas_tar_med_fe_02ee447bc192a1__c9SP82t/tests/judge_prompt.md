You are an expert code reviewer evaluating a refactored version of code that originally contained a "feature_envy" code smell.

## Context
- **Smell Type**: feature_envy
- **Smell Description**: A function that is more interested in data from other classes than its own, indicating misplaced behavior.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Change Analysis

### 1. New function `prepare_combine_series_pair` in `pandas/core/common.py`

**What it does**: Extracts logic from `DataFrame.combine` to handle NA mask computation, fill value application, and the overwrite-check that determines whether to skip combining a column.

**Significance**: **CRITICAL** - This is one of the two core manifestations of feature envy. This function operates almost entirely on Series objects (calling `isna()`, checking `.all()`, using `.copy()`, performing indexing with `this_col[this_mask]`). It exists in a utility module (`common.py`) but is deeply concerned with Series-specific operations.

**What it degrades**: 
- **Cohesion**: The function breaks apart cohesive DataFrame.combine logic and scatters it across modules
- **Coupling**: Creates a new dependency from `common.py` to Series internals (needs to import from `pandas.core.dtypes.missing`)
- **Encapsulation**: Exposes DataFrame internals (how it handles NA masks, fill values) as a public utility when it should be private behavior
- **API surface**: Adds a utility function to a common module that's really only relevant to DataFrame.combine operations

### 2. New function `resolve_combine_column_dtype` in `pandas/core/dtypes/cast.py`

**What it does**: Extracts dtype resolution logic from `DataFrame.combine`, handling both the special case (column only in `other`) and common case (column in both DataFrames) by calling `find_common_type()` and performing `.astype()` conversions.

**Significance**: **CRITICAL** - This is the second core manifestation of feature envy. The function is entirely about manipulating Series objects (calling `.dtype`, `.astype()`) and inspecting DataFrame structure (`col not in self_columns`). It's placed in a casting module but is really concerned with Series behavior.

**What it degrades**:
- **Cohesion**: Splits dtype negotiation logic away from the combine operation where it belongs
- **Feature Envy (direct)**: The function signature takes `series`, `other_series`, but then obsessively accesses their properties (`.dtype`) and methods (`.astype()`), manipulating them from outside rather than having them handle their own concerns
- **Single Responsibility**: The `cast.py` module should handle type casting utilities, not orchestrate DataFrame column comparison logic (checking `col not in self_columns`)
- **Coupling**: Creates tight coupling between cast utilities and DataFrame column semantics

### 3. Import of `resolve_combine_column_dtype` in `pandas/core/frame.py`

**What it does**: Adds the import statement to bring in the newly extracted function.

**Significance**: **MODERATE** - This is a supporting change that enables the refactoring. The import itself isn't the problem, but it reflects the architectural issue.

**What it degrades**:
- **Module dependencies**: Makes DataFrame depend on a cast utility that embeds DataFrame-specific logic, creating a circular dependency in spirit

### 4. Replacement of inline logic in `DataFrame.combine` with calls to extracted functions

**What it does**: Replaces 30+ lines of inline logic with two function calls:
- `com.prepare_combine_series_pair(...)` to handle NA masks and fill values
- `resolve_combine_column_dtype(...)` to handle dtype resolution

**Significance**: **CRITICAL** - This is where the original cohesive method loses its data and behavior. The DataFrame.combine method now delegates intimate knowledge of its Series objects to external utilities.

**What it degrades**:
- **Information hiding**: DataFrame.combine previously kept its Series manipulation logic private; now it exposes Series objects to external functions that manipulate them
- **Cohesion**: The combine operation is now fragmented across three locations (frame.py, common.py, cast.py)
- **Readability**: To understand the full combine operation, developers must jump between three files instead of reading one method
- **Locality of behavior**: Related logic that operates on the same data is now separated

### 5. Removal of local variables and logic (`this_dtype`, `other_dtype`, `this_mask`, `other_mask`, `do_fill`, inline dtype resolution)

**What it does**: Removes the inline computation of masks, dtypes, and fill logic that was previously part of DataFrame.combine.

**Significance**: **MODERATE** - This is the consequence of the extraction, not the cause. The removal itself indicates loss of cohesion.

**What it degrades**:
- **Method completeness**: DataFrame.combine is now incomplete on its own; it's a coordinator rather than a complete implementation

## Overall Smell Pattern

**Feature Envy Manifestation**: The smell emerges when behavior that should belong to the DataFrame class (or be private helper methods within it) is extracted into utility functions in unrelated modules (`common.py` and `cast.py`). These utility functions then "envy" the features of the Series class—they obsessively access Series properties (`.dtype`, `.copy()`) and methods (`.astype()`, indexing operations) to manipulate them from the outside.

**Design Principle Violated**: 
- **Tell, Don't Ask**: The extracted functions ask Series objects for their data (dtype, NA masks) and then manipulate them, rather than telling Series objects what to do
- **High Cohesion, Low Coupling**: The refactoring reduces cohesion (splits related logic) and increases coupling (common.py and cast.py now depend on Series internals)
- **Information Hiding**: DataFrame internals are exposed to utility modules that shouldn't need to know about combine-specific logic

**The Core Problem**: Both `prepare_combine_series_pair` and `resolve_combine_column_dtype` are more interested in Series data than in their nominal purposes (common utilities and casting utilities). They belong as private methods of DataFrame, not as public utilities.

## Severity Ranking (Most to Least Important)

1. **CRITICAL**: `resolve_combine_column_dtype` in cast.py - This is the worst offender because it mixes DataFrame structural logic (`col not in self_columns`) with Series manipulation (`.dtype`, `.astype()`), and places it in a completely inappropriate module (casting utilities)

2. **CRITICAL**: `prepare_combine_series_pair` in common.py - Nearly as bad; it's deeply concerned with Series internals (NA masks, copying, indexing) but lives in a common utilities module

3. **CRITICAL**: Replacement logic in DataFrame.combine - This is the enabling change that creates the fragmentation and loss of cohesion

4. **MODERATE**: Import statement - Supporting infrastructure for the smell

5. **MODERATE**: Removal of inline variables - Consequence rather than cause

## What Was Degraded Overall

**Concrete Quality Degradation**:

1. **Cohesion (Severe)**: A cohesive 30-line method is fragmented across three modules. Related logic that operates on the same data structures is now separated by module boundaries.

2. **Coupling (Severe)**: 
   - `common.py` now couples to Series internals and `pandas.core.dtypes.missing`
   - `cast.py` now couples to DataFrame column structure and Series manipulation
   - These utilities become less reusable because they embed DataFrame.combine-specific logic

3. **Encapsulation (Moderate)**: DataFrame internal logic (how it handles NA during combine) is now public API in utility modules

4. **Maintainability (Severe)**: 
   - Bug fixes or enhancements to combine logic now require changes across three files
   - Understanding the combine operation requires reading three separate functions in different modules
   - The logic flow is non-local and harder to trace

5. **Module Responsibility (Severe)**:
   - `common.py` should contain genuinely common utilities, not DataFrame.combine-specific helpers
   - `cast.py` should handle type casting, not DataFrame column comparison logic

6. **Testability (Moderate)**: While the extracted functions are individually testable, the integration testing becomes more complex, and the public API surface increases unnecessarily

## Key Evaluation Signals

**To distinguish a thorough fix from a superficial one, look for:**

1. **Location of logic**: Does the fix move `prepare_combine_series_pair` and `resolve_combine_column_dtype` back into DataFrame (as private methods or inline)? If they remain in common.py/cast.py, the smell persists.

2. **Cohesion restoration**: Is the combine logic once again co-located? A thorough fix reunites the scattered logic so it can be understood in one place.

3. **Coupling reduction**: Does the fix eliminate the dependency from common.py/cast.py to Series manipulation? Check if these modules still import Series-related utilities.

4. **API surface**: Does the fix remove these functions from public utility modules? They should not be exposed as utilities if they're only used by DataFrame.combine.

5. **Tell, Don't Ask principle**: Does the fix reduce the amount of Series property access (`.dtype`, masking) from external functions? Ideally, Series objects should be told what to do rather than having their data extracted and manipulated externally.

6. **Method completeness**: Can DataFrame.combine be understood largely on its own, or does it remain a thin coordinator of external utilities?

**Red flags in a superficial fix**:
- Moving the functions but keeping them as public utilities
- Renaming without relocating
- Adding more abstraction layers without addressing the coupling
- Keeping the fragmentation across modules
- Documentation that tries to justify the utility placement rather than fixing the architecture

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
