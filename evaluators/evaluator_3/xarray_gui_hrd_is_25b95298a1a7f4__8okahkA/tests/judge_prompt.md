You are an expert code reviewer evaluating a refactored version of code that originally contained a "interface_segregation" code smell.

## Context
- **Smell Type**: interface_segregation
- **Smell Description**: When interfaces are too large or force implementing classes to depend on methods they don't use, violating interface segregation principle.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Change Analysis

### 1. New Protocol: `CoordinateTransformable` (xarray/core/types.py)
**What it does**: Defines a Protocol with four abstract methods that any coordinate-transformable object must implement: `_compute_coord_alignment_key()`, `_resolve_multi_var_coord_state()`, `_validate_single_var_bounds()`, and `_get_coord_lifecycle_hooks()`.

**Significance**: **Critical**. This is the root cause of the interface segregation violation. The protocol forces all implementers to provide all four methods, even though the documentation explicitly states different methods are for different use cases ("For multi-variable containers" vs "For single-variable containers").

**What it degrades**: 
- **Interface Segregation Principle**: Forces classes to depend on methods they don't use
- **Cohesion**: Bundles unrelated responsibilities (single-var and multi-var operations) into one interface
- **API clarity**: The protocol tries to serve two different abstractions (Dataset vs DataArray) through one interface

### 2. Four Abstract Methods Added to `DataWithCoords` (xarray/core/common.py)
**What it does**: Adds four new abstract methods to the base class that mirror the Protocol:
- `_compute_coord_alignment_key()`: Returns strategy identifier
- `_resolve_multi_var_coord_state()`: For multi-variable coordination
- `_validate_single_var_bounds()`: For single-variable validation
- `_get_coord_lifecycle_hooks()`: Returns lifecycle hooks (has default implementation)

**Significance**: **Critical**. This forces all subclasses (Dataset, DataArray, and any future implementations) to implement methods they may not need. DataArray doesn't need multi-var logic, Dataset doesn't need single-var validation in the same way.

**What it degrades**:
- **Unnecessary coupling**: DataArray must implement `_resolve_multi_var_coord_state()` even though it's inherently single-variable
- **API bloat**: Every subclass now has 3+ additional abstract methods to implement
- **Cognitive load**: Developers must understand all four methods even if their class only needs 1-2

### 3. Template Method Implementation: `_prepare_coord_operation()` (xarray/core/common.py)
**What it does**: Provides a concrete method that serves as the "template method" entry point, dispatching to strategy functions based on the alignment key.

**Significance**: **Moderate**. While this implements the Template Method pattern, it's built on top of the segregation violation. The dispatch itself isn't the problem—the fat interface feeding into it is.

**What it degrades**:
- **Indirection**: Adds another layer of dispatch complexity
- **Discoverability**: The actual behavior is hidden behind strategy registration

### 4. Strategy Registry System (xarray/core/ops.py)
**What it does**: Creates a registry pattern with `_coord_operation_strategies` dict, `register_coord_strategy()` decorator, and two registered strategies (`_multi_var_coord_strategy` and `_single_var_coord_strategy`).

**Significance**: **Moderate**. The strategy pattern itself is reasonable, but it's solving a problem created by the fat interface. The strategies explicitly call different methods (`_resolve_multi_var_coord_state()` vs `_validate_single_var_bounds()`), proving they need different interfaces.

**What it degrades**:
- **Module coupling**: Creates tight coupling between ops.py and the base class structure
- **Hidden control flow**: The decorator registration makes behavior less explicit

### 5. DataArray Implementation (xarray/core/dataarray.py)
**What it does**: DataArray implements all four abstract methods:
- `_compute_coord_alignment_key()` returns "single_var"
- `_resolve_multi_var_coord_state()` provides a multi-var implementation even though DataArray is single-variable
- `_validate_single_var_bounds()` provides actual single-var validation
- `_get_coord_lifecycle_hooks()` returns a dummy hook

**Significance**: **Critical as evidence**. This reveals the smell in action: DataArray is forced to implement `_resolve_multi_var_coord_state()` despite being inherently single-variable. The implementation tries to make sense of it, but it's conceptually wrong—a DataArray iterating over "coord_names" and checking multiple coordinates is pretending to be multi-variable.

**What it degrades**:
- **Semantic clarity**: DataArray now has multi-variable coordination methods that make no conceptual sense
- **Dead/dubious code**: `_resolve_multi_var_coord_state()` on a single-variable container is misleading

### 6. Dataset Implementation (xarray/core/dataset.py)
**What it does**: Dataset implements all four abstract methods:
- `_compute_coord_alignment_key()` returns "multi_var"
- `_resolve_multi_var_coord_state()` provides genuine multi-var logic
- `_validate_single_var_bounds()` provides validation that can check a single var or all vars
- `_get_coord_lifecycle_hooks()` returns a dummy hook

**Significance**: **Critical as evidence**. Dataset is forced to implement `_validate_single_var_bounds()`, which is conceptually a single-variable concern. The implementation tries to adapt it (checking all variables if var_name is None), but this blurs responsibilities.

**What it degrades**:
- **Single Responsibility**: Dataset now has to worry about "single variable bounds" when it's a multi-variable container
- **Naming confusion**: "single_var_bounds" on a Dataset doesn't make semantic sense

### 7. Call Sites: `_prepare_coord_operation()` Added (xarray/core/common.py)
**What it does**: Three call sites added before coordinate operations:
- In `squeeze()`: calls with "squeeze" operation and dims set
- In `assign_coords()`: calls with "assign" operation and coord keys
- In `where()`: calls with "where" operation and no coord names

**Significance**: **Minor**. These are just usage of the new infrastructure. They show the system is being integrated, but they're not the source of the smell.

**What it degrades**:
- **Performance**: Adds validation/coordination overhead to operations
- **Call complexity**: Each operation now has an extra preprocessing step

### 8. `Closeable` Protocol (xarray/core/types.py)
**What it does**: Defines a minimal protocol for resource cleanup with `close()` and `set_close()`.

**Significance**: **Negligible**. This appears unrelated to the coordinate transformation smell. It's actually a good example of interface segregation—small, focused protocol.

**What it degrades**: Nothing significant; possibly adds minor API surface.

## Overall Smell Pattern

This diff implements an **over-generalized interface** that violates the Interface Segregation Principle by forcing implementers to depend on methods they don't need. Specifically:

1. **Two distinct responsibilities** are bundled into one interface:
   - Multi-variable coordination (for Dataset): `_resolve_multi_var_coord_state()`
   - Single-variable validation (for DataArray): `_validate_single_var_bounds()`

2. **The "fat" base class**: `DataWithCoords` requires all subclasses to implement methods for both responsibilities, even though each subclass only needs one.

3. **Evidence of the violation**: 
   - DataArray must implement `_resolve_multi_var_coord_state()` despite being single-variable
   - Dataset must implement `_validate_single_var_bounds()` despite being multi-variable
   - Both provide implementations that try to make sense of inapplicable methods

4. **The core violation**: Clients (DataArray and Dataset) are forced to depend on interfaces they don't use. This increases coupling, reduces cohesion, and makes the API harder to understand and maintain.

The strategy pattern and template method are reasonable design patterns, but they're built on top of a flawed interface design. The strategies explicitly call different methods, proving that different abstractions are needed.

## Severity Ranking (Most to Least Important)

1. **CRITICAL: Four abstract methods in DataWithCoords** - This is the root cause. Making these abstract forces all subclasses to implement all methods regardless of relevance.

2. **CRITICAL: CoordinateTransformable Protocol** - Codifies the violation in a reusable protocol, potentially spreading the smell to other parts of the codebase.

3. **CRITICAL (Evidence): DataArray/Dataset implementations** - These show the smell in action, with each class implementing methods it shouldn't need.

4. **MODERATE: Strategy registry system** - Enables the dispatch but reveals through its design (two different strategies calling different methods) that different interfaces are needed.

5. **MODERATE: Template method (_prepare_coord_operation)** - Orchestrates the violation but isn't itself the problem.

6. **MINOR: Call sites** - Just usage of the flawed infrastructure.

7. **NEGLIGIBLE: Closeable protocol** - Unrelated to the smell; actually demonstrates good interface design.

## What Was Degraded Overall

**Concrete impacts:**

1. **Interface Segregation Principle**: Violated by forcing all subclasses to implement 4 methods when each needs only 2-3.

2. **Cohesion**: DataWithCoords now mixes concerns for single-variable and multi-variable coordination in one interface.

3. **Semantic Clarity**: DataArray has multi-variable methods; Dataset has single-variable methods. Neither makes conceptual sense.

4. **API Surface**: Each subclass gains 3-4 new methods, increasing the API footprint by ~15-20 methods across the hierarchy.

5. **Maintainability**: 
   - Future implementers must understand and implement all methods
   - Testing burden increases (must test irrelevant method implementations)
   - Documentation must explain why methods exist that don't apply

6. **Coupling**: Tight coupling between base class, strategy registry, and concrete implementations.

7. **Code Clarity**: Developers reading DataArray see `_resolve_multi_var_coord_state()` and must understand it's forced by the base class but not truly applicable.

## Key Evaluation Signals

**What should matter most when evaluating a fix:**

1. **Interface segregation**: Does the fix split the fat interface into focused, role-specific interfaces? The ideal fix would have separate interfaces/protocols for multi-var and single-var concerns, or use composition instead of inheritance.

2. **Method relevance**: After the fix, do DataArray and Dataset only implement methods relevant to their nature? DataArray should not have `_resolve_multi_var_coord_state()`, and Dataset should not have `_validate_single_var_bounds()` (unless genuinely needed).

3. **Abstraction alignment**: Do the abstractions match the domain? Single-variable operations should not appear in multi-variable containers and vice versa.

4. **Protocol/interface splitting**: A thorough fix would likely involve:
   - Splitting `CoordinateTransformable` into two protocols (or removing it entirely)
   - Making some methods in DataWithCoords optional or using delegation instead of inheritance
   - Possibly using composition (has-a) instead of inheritance (is-a) for strategy selection

5. **Reduced coupling**: The fix should reduce the number of methods each class must implement and understand.

**Distinguishing thorough from superficial fixes:**

- **Superficial**: Just making the methods optional/non-abstract but leaving them in the base class. This reduces forced implementation but doesn't fix the conceptual violation.
  
- **Superficial**: Providing no-op implementations in the base class. This hides the problem but doesn't fix the fat interface.

- **Thorough**: Removing irrelevant methods from each class entirely, splitting responsibilities into separate concerns, and using composition or separate protocols where inheritance creates coupling.

- **Thorough**: Each class should only know about methods relevant to its coordination strategy, with no "dead" or "not applicable" methods.

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

### Context for "interface_segregation"

**Focus:** Whether the fat interface is correctly split into focused, cohesive interfaces and unnecessary stubs are removed.
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
