You are an expert code reviewer evaluating a refactored version of code that originally contained a "data_clumps" code smell.

## Context
- **Smell Type**: data_clumps
- **Smell Description**: Groups of variables that are frequently passed together, suggesting poor encapsulation or missing class abstraction.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

This diff introduces a "data_clumps" code smell by breaking apart cohesive data and creating several utility functions that pass around the same groups of parameters repeatedly. Let me analyze each change:

## Individual Changes

### 1. `compute_plot_variables(x_var, y_var, hue_var)` in `_base.py`
**What it does**: Extracts the logic for determining which variables are needed for a bivariate plot. It builds a list containing x_var, y_var (if different from x_var), and optionally hue_var.

**Significance**: **Critical** - This is a core manifestation of the smell. The function takes three primitive parameters that represent a conceptual unit (plot variable configuration) but aren't encapsulated.

**What it degrades**: 
- Cohesion: The logic that determines plot variables is separated from the context where it's used
- Semantic clarity: The relationship between x_var, y_var, and hue_var isn't captured in a type
- API surface: Adds a new module-level function that increases cognitive load

### 2. `build_hue_kwargs(hue_data, hue_order, palette)` in `palettes.py`
**What it does**: Bundles three hue-related parameters into a dictionary for passing to plotting functions.

**Significance**: **Critical** - Another key smell indicator. These three parameters (`hue_data`, `hue_order`, `palette`) consistently travel together, suggesting they should be a cohesive object.

**What it degrades**:
- Cohesion: Hue configuration is scattered across three separate primitive values
- Module responsibility: Palettes module now has utility functions for building kwargs dictionaries, mixing concerns
- Type safety: Returns a plain dict instead of a typed structure

### 3. `resolve_bivariate_data(data, x_var, y_var, axes_vars, dropna)`
**What it does**: Filters the dataframe based on axes_vars and optionally drops NA values, returning x, y, and the filtered data.

**Significance**: **Moderate** - This function exhibits the smell by taking 5 parameters, several of which are related (x_var, y_var, axes_vars all describe variable selection).

**What it degrades**:
- Parameter coupling: axes_vars is derived from x_var and y_var via `compute_plot_variables`, yet all are passed separately
- Data flow clarity: The relationship between input parameters and the three-tuple return is obscure

### 4. `get_hue_values(plot_data, hue_var)`
**What it does**: Extracts hue column from plot_data if hue_var is not None.

**Significance**: **Minor** - A trivial wrapper that doesn't add value.

**What it degrades**:
- API bloat: Adds a function for a one-liner conditional
- Indirection: Makes code harder to follow by hiding simple logic

### 5. `is_seaborn_func(func)`
**What it does**: Checks if a function's module starts with "seaborn".

**Significance**: **Minor** - Not directly related to the data clump smell, just DRY refactoring.

**What it degrades**: Nothing significantly; this is reasonable extraction.

### 6. Modified `_plot_bivariate` signature: added `hue_var, dropna` parameters
**What it does**: Explicitly passes hue_var and dropna to _plot_bivariate instead of accessing them from self.

**Significance**: **Critical** - This is where the smell becomes apparent in practice. The method now takes 6 explicit parameters, and hue_var + dropna are systematically passed through multiple layers.

**What it degrades**:
- Method signatures: Longer parameter lists that obscure the essential parameters
- Parameter drilling: Creates a pattern where data must be passed through multiple levels
- Encapsulation: Instance variables that were properly encapsulated in `self` are now threaded as parameters

### 7. Modified `_plot_bivariate_iter_hue` signature: added `hue_var, dropna` parameters
**What it does**: Same as above - adds explicit parameters instead of using instance state.

**Significance**: **Critical** - Compounds the parameter drilling problem.

**What it degrades**: Same as #6

### 8. Call site modifications in `map_offdiag`
**What it does**: Extracts `hue_var` and `dropna` into local variables before the loop, then passes them to `_plot_bivariate`.

**Significance**: **Moderate** - Shows the pattern in action: data that was previously accessed via self is now manually threaded through calls.

**What it degrades**:
- Local reasoning: More variables to track in the method
- Coupling: The caller must know which instance variables the callee needs

### 9. Refactored `_plot_bivariate` body
**What it does**: Replaces inline logic with calls to the new utility functions (`compute_plot_variables`, `resolve_bivariate_data`, `get_hue_values`, `build_hue_kwargs`).

**Significance**: **Critical** - This demonstrates how the data clump manifests: the same group of variables (x_var, y_var, hue_var, dropna) flows through multiple function calls in sequence.

**What it degrades**:
- Local coherence: What was straightforward inline code is now scattered across function calls
- Debuggability: Multiple function calls make stack traces deeper and stepping harder
- Readability: The logical flow is obscured by indirection

## Overall Smell Pattern

The "data_clumps" smell here manifests as **groups of primitive parameters that consistently travel together**:

1. **Plot variable clump**: `x_var`, `y_var`, `hue_var` - these three always appear together and define the variables for a plot
2. **Hue configuration clump**: `hue_data`, `hue_order`, `palette` - these three configure hue mapping
3. **Data processing clump**: `axes_vars`, `dropna` - these control data filtering

The violated design principles:
- **Data cohesion**: Related data items should be grouped into objects
- **Primitive obsession**: Using primitives instead of small objects to represent concepts
- **Information hiding**: Instance variables are exposed as parameters, breaking encapsulation
- **Single Responsibility**: Utility functions are created that each handle one aspect of a larger cohesive operation

## Severity Ranking (Most to Least Important)

1. **`build_hue_kwargs()` and its usage** - The clearest data clump: three parameters always used together
2. **`_plot_bivariate` and `_plot_bivariate_iter_hue` signature changes** - Parameter drilling that breaks encapsulation
3. **`compute_plot_variables()` and its usage** - Three variables that represent a single concept split apart
4. **`resolve_bivariate_data()`** - Takes redundant/derived parameters
5. **Refactored `_plot_bivariate` body** - Consequence of the above changes
6. **`map_offdiag` modifications** - Symptom of parameter drilling
7. **`get_hue_values()`** - Trivial wrapper, minor issue
8. **`is_seaborn_func()`** - Not really part of the data clump smell

## What Was Degraded Overall

**Cohesion**: The code has been fragmented. What was cohesive logic within methods is now scattered across multiple utility functions. Related data items (like hue configuration) aren't bundled into meaningful objects.

**Encapsulation**: Instance variables (`self._hue_var`, `self._dropna`) are exposed as parameters and threaded through method calls, violating information hiding.

**Maintainability**: 
- Adding a new plot variable parameter would require touching 5+ functions
- Understanding the data flow requires tracking parameters across multiple function calls
- The proliferation of small utility functions increases cognitive load

**Type Safety**: Using primitive types (strings, bools) and dictionaries instead of typed objects means no compile-time checking of parameter relationships.

**API Surface**: Four new module-level utility functions expand the public API without clear benefit, increasing the learning curve.

**Readability**: The call chains obscure what's happening. Compare the original inline code to the new version with `compute_plot_variables`, `resolve_bivariate_data`, `get_hue_values`, and `build_hue_kwargs` - it's much harder to follow.

## Key Evaluation Signals

A thorough fix should:

1. **Introduce cohesive objects**: Replace parameter groups with objects (e.g., a `PlotVariables` class containing x_var, y_var, hue_var; a `HueConfig` class with hue_data, order, palette)

2. **Restore encapsulation**: Stop passing `hue_var` and `dropna` as parameters when they're available as instance variables. Methods should use `self._hue_var` and `self._dropna` directly.

3. **Eliminate redundant parameters**: Functions like `resolve_bivariate_data` shouldn't take both `x_var, y_var` AND `axes_vars` when the latter is derived from the former.

4. **Remove trivial utilities**: Functions like `get_hue_values` and `is_seaborn_func` should be inlined or used only if they truly add abstraction value.

5. **Reduce parameter counts**: Method signatures should become shorter, with related data bundled into objects.

6. **Preserve or improve locality**: The logic in `_plot_bivariate` should be more readable, not less, with cohesive objects making the data flow clearer.

A superficial fix might just rename things or consolidate one or two functions. A real fix would recognize that the core problem is **primitive obsession** and **lack of cohesive data structures**, addressing it by introducing appropriate abstractions.

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
