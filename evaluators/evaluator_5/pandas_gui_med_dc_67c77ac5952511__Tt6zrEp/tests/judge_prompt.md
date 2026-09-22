You are an expert code reviewer evaluating a refactored version of code that originally contained a "data_clumps" code smell.

## Context
- **Smell Type**: data_clumps
- **Smell Description**: Groups of variables that are frequently passed together, suggesting poor encapsulation or missing class abstraction.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

# Data Clumps Code Smell Analysis

## Individual Changes Assessment

### 1. Addition of `_resolve_axis_display_config()` in `_core.py` (lines 639-667)

**What it does**: This function accepts 8 individual parameters related to axis display configuration (x_ticks, y_ticks, x_lim, y_lim, x_label, y_label, tick_fontsize, tick_rot), performs minimal processing (converting lists to tuples for limits), and returns all 8 values as a tuple.

**Significance**: **CRITICAL** - This is a primary manifestation of the data clumps smell. The function exists solely to shuttle 8 related parameters that should logically be grouped together. It provides almost no real functionality beyond type coercion.

**What it degrades**: 
- **Cohesion**: The function has no real purpose except to pass data through
- **API surface**: Adds unnecessary complexity with 8 parameters that always travel together
- **Encapsulation**: These related axis properties aren't wrapped in a cohesive abstraction
- **Code clarity**: The tuple unpacking pattern obscures the relationship between these parameters

### 2. Call to `_resolve_axis_display_config()` in `PlotAccessor.__call__()` (lines 996-1015)

**What it does**: Extracts 8 kwargs values, passes them to `_resolve_axis_display_config()`, then immediately unpacks the returned tuple back into the same 8 kwargs keys.

**Significance**: **CRITICAL** - This demonstrates the worst aspect of data clumps: extracting 8 values from a dictionary, passing them through a function, and putting them right back. This is pure ceremony with no real value.

**What it degrades**:
- **Readability**: The extract-call-unpack pattern spans 22 lines for minimal actual work
- **Maintainability**: Any change to the axis parameters requires updates in 3 places (extraction, function signature, unpacking)
- **Performance**: Unnecessary dict lookups and tuple packing/unpacking

### 3. Addition of `_prepare_axis_display()` in `_matplotlib/__init__.py` (lines 58-89)

**What it does**: Another function accepting the same 8 parameters (with slightly different names), performs tuple normalization, and returns them as a dictionary.

**Significance**: **CRITICAL** - This is a duplicate of the previous pattern but with different parameter names and returning a dict instead of tuple. This duplication highlights how the data clump is spreading through the codebase.

**What it degrades**:
- **DRY principle**: Duplicates the logic from `_resolve_axis_display_config()` with different naming
- **Consistency**: Uses different parameter names (tick_x_positions vs x_ticks, range_x vs x_lim, etc.)
- **Coupling**: Creates tight coupling between the frontend and backend through this shared parameter structure

### 4. Call to `_prepare_axis_display()` in `plot()` function (lines 105-116)

**What it does**: Pops 8 values from kwargs, passes to `_prepare_axis_display()`, gets back a dict, and updates kwargs with that dict.

**Significance**: **MODERATE** - This is the "pop-transform-update" pattern that's slightly better than the previous extract-unpack pattern, but still demonstrates the data clump anti-pattern.

**What it degrades**:
- **Code clarity**: The pop-call-update cycle is confusing
- **Maintainability**: The transformation from one naming scheme to another adds cognitive load

### 5. Addition of `_init_axis_display()` method in `MPLPlot` (lines 282-314)

**What it does**: Extracts the initialization logic for the same 8 axis display parameters from `__init__()` into a separate method.

**Significance**: **MODERATE** - This is a refactoring that moves code without addressing the underlying problem. The 8 parameters are still passed individually.

**What it degrades**:
- **Simplicity**: Adds an extra method call for initialization that doesn't add clear value
- **Discoverability**: The initialization logic is now hidden in a helper method

### 6. Refactoring in `MPLPlot.__init__()` (lines 213-226)

**What it does**: Replaces direct assignment of 8 instance variables with a call to `_init_axis_display()`.

**Significance**: **MINOR** - This is a mechanical refactoring that doesn't change the fundamental problem.

**What it degrades**:
- **Directness**: The initialization is now indirect through a method call

### 7. Addition of `_decorate_axis()` static method in `MPLPlot` (lines 775-811)

**What it does**: Takes an axes object and 6 of the 8 axis parameters, applies them to the axes.

**Significance**: **MODERATE** - This is actually a reasonable extraction that reduces code in `_adorn_subplots()`, but it still suffers from the same parameter list problem (6 parameters).

**What it degrades**:
- **Parameter lists**: Still has 7 total parameters (ax + 6 axis settings)
- **Cohesion**: The 6 axis settings would be better as a single object

### 8. Refactoring in `_adorn_subplots()` (lines 830-837)

**What it does**: Replaces inline axis decoration code with a call to `_decorate_axis()`.

**Significance**: **MINOR** - This is a reasonable refactoring that reduces duplication, though it perpetuates the data clump.

**What it degrades**:
- Minimal degradation; this is actually an improvement in some ways

## Overall Smell Pattern

**Core Pattern**: The data clumps smell manifests through 8 axis display parameters (xticks, yticks, xlim, ylim, xlabel, ylabel, fontsize, rot) that are repeatedly passed together as individual parameters across multiple functions and layers. These parameters clearly represent a cohesive concept ("axis display configuration") but are never encapsulated into a proper abstraction.

**Design Principles Violated**:
1. **Information Hiding**: The internal structure of axis configuration is exposed everywhere
2. **High Cohesion**: Related data isn't grouped together
3. **Low Coupling**: Functions are coupled through shared parameter lists rather than abstractions
4. **Single Responsibility**: Functions like `_resolve_axis_display_config()` exist only to shuttle data
5. **DRY**: The same parameter lists are repeated across multiple layers
6. **Tell, Don't Ask**: Code extracts individual values rather than passing cohesive objects

## Severity Ranking (Most to Least Important)

1. **CRITICAL: `_resolve_axis_display_config()` and its call site** - This is the root cause showing pure data clump with no real logic
2. **CRITICAL: `_prepare_axis_display()` and its call site** - Duplication of the same pattern with inconsistent naming
3. **MODERATE: `_decorate_axis()` method** - Perpetuates the parameter list problem but at least does real work
4. **MODERATE: `_init_axis_display()` method** - Moves code around without fixing the fundamental issue
5. **MINOR: Changes to `__init__()` and `_adorn_subplots()`** - Mechanical refactorings that are neutral or slightly positive

## What Was Degraded Overall

**Concrete Impacts**:

1. **Maintainability**: Adding a new axis parameter now requires changes in 6+ locations across 3 files
2. **Type Safety**: Tuple unpacking and dict operations lose type information and parameter names
3. **Code Volume**: Added ~100 lines of code that provide minimal actual value
4. **Cognitive Load**: Developers must track 8 individual parameters through multiple transformation steps
5. **API Complexity**: Each function/method that touches axis configuration has 6-8 extra parameters
6. **Testing Surface**: Each new function requires tests, multiplying test maintenance
7. **Consistency**: Different layers use different names for the same concepts (tick_x_positions vs x_ticks)
8. **Encapsulation**: No single place "owns" the axis configuration concept

## Key Evaluation Signals for Fixes

**What matters MOST**:

1. **Encapsulation of the 8 parameters**: A proper fix MUST introduce a class/dataclass/named tuple that groups these 8 related parameters. Functions should accept/return this object, not individual parameters.

2. **Elimination of intermediate shuttle functions**: `_resolve_axis_display_config()` and `_prepare_axis_display()` should not exist in their current form. If axis configuration objects exist, these functions become unnecessary or become proper transformation logic.

3. **Consistent naming across layers**: The fix should use consistent names throughout (not tick_x_positions in one place and x_ticks in another).

4. **Reduced parameter lists**: Function signatures should shrink from 8-10 parameters to 2-4 (the axis config object + a few others).

**What distinguishes thorough from superficial**:

- **Superficial**: Just wrapping the parameters in a dict/namespace without semantic meaning
- **Thorough**: Creating a proper AxisConfiguration class with validation, defaults, and transformation methods

- **Superficial**: Only fixing one layer (e.g., just the matplotlib backend)
- **Thorough**: Consistent abstraction from PlotAccessor through to MPLPlot

- **Superficial**: Keeping the extract-call-unpack patterns but with objects instead of primitives
- **Thorough**: Passing the configuration object through layers without unpacking/repacking

- **Superficial**: Maintaining the duplicate logic in multiple functions
- **Thorough**: Centralizing axis configuration logic in the configuration object itself

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

### Context for "data_clumps"

**Focus:** Whether all instances of the data clump are identified across files and replaced with a well-designed abstraction.
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
