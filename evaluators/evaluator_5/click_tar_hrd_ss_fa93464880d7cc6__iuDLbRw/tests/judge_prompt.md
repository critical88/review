You are an expert code reviewer evaluating a refactored version of code that originally contained a "shotgun_surgery" code smell.

## Context
- **Smell Type**: shotgun_surgery
- **Smell Description**: A change that requires making small modifications in many different classes or files, indicating scattered responsibilities.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Changes Analysis

### 1. **`src/click/globals.py` - Input normalizer registry**
   - **What it does**: Introduces a global registry (`_input_normalizers`) with three functions: `register_input_normalizer()`, `get_input_normalizers()`, and `apply_input_normalizers()`. This creates a centralized mechanism for managing string transformation functions that will be applied to prompt input.
   - **Significance**: **CRITICAL** - This is the root cause of the smell. By creating a global registry that accepts registrations from anywhere in the codebase, it necessitates scattered changes across multiple modules.
   - **Degradation**: Creates tight coupling between unrelated modules through a shared global state mechanism. Violates Single Responsibility Principle by making `globals.py` responsible for input normalization coordination.

### 2. **`src/click/_compat.py` - Platform line endings normalizer**
   - **What it does**: Defines `_normalize_platform_line_endings()` and registers it via `_register_platform_normalizer()` which is called at module import time.
   - **Significance**: **CRITICAL** - This demonstrates the shotgun surgery pattern in action. A platform compatibility module now has to import and interact with the globals registry.
   - **Degradation**: Breaks module cohesion - `_compat.py` should handle compatibility abstractions, not register callbacks into a global pipeline. Creates import-time side effects that make module initialization order significant.

### 3. **`src/click/core.py` - Control characters normalizer**
   - **What it does**: Defines `_normalize_control_chars()` and registers it immediately at module level (priority=15).
   - **Significance**: **CRITICAL** - Core command processing logic now has registration side effects. The import adds `register_input_normalizer` to the module's dependencies.
   - **Degradation**: The core module should focus on command/context abstractions, not input sanitization. Module-level registration creates hidden dependencies and makes testing harder.

### 4. **`src/click/decorators.py` - Null bytes normalizer**
   - **What it does**: Defines `_normalize_null_bytes()` and registers it at module level (priority=40).
   - **Significance**: **CRITICAL** - The decorator module now participates in the input normalization pipeline despite having no logical relationship to input processing.
   - **Degradation**: Decorators should be about function wrapping and metadata, not input sanitization. This violates the module's cohesive purpose.

### 5. **`src/click/types.py` - Unicode surrogates normalizer**
   - **What it does**: Defines `_normalize_unicode_surrogates()` and registers it at module level (priority=30).
   - **Significance**: **CRITICAL** - Type conversion logic now has registration side effects.
   - **Degradation**: While unicode handling is somewhat related to types, the registration mechanism forces coupling to the globals module. The type system should be self-contained.

### 6. **`src/click/utils.py` - Trailing whitespace normalizer**
   - **What it does**: Defines `_normalize_trailing_whitespace()` and registers it at module level (priority=20).
   - **Significance**: **CRITICAL** - A utility module with general-purpose helpers now participates in a specific input processing pipeline.
   - **Degradation**: Utils module loses its generic character. Adding import of `register_input_normalizer` creates dependency on globals.

### 7. **`src/click/termui.py` - Application point**
   - **What it does**: Imports `apply_input_normalizers` and calls it on prompt input with a single line: `value = apply_input_normalizers(value)`.
   - **Significance**: **MODERATE** - This is the actual usage point, but it's relatively localized. The change itself is reasonable.
   - **Degradation**: Minimal local degradation, but it's the point that makes all the scattered registrations necessary. Creates implicit dependency on all modules that register normalizers being imported first.

### 8. **`src/click/exceptions.py` - Error message normalizers**
   - **What it does**: Introduces a separate `_error_message_normalizers` list with its own registration mechanism, complete with a comment explaining it's "intentionally NOT part of the input normalizer registry."
   - **Significance**: **MINOR** - This is interesting because it shows awareness of the problem but creates a parallel, disconnected system rather than fixing the architecture.
   - **Degradation**: Adds complexity and inconsistency. The comment indicates the developer recognized the registry pattern might be overreaching, but chose to duplicate rather than refactor.

### 9. **`src/click/testing.py` - Test input normalizer**
   - **What it does**: Defines `_test_input_normalizer()` with a comment explaining it's NOT registered in the global pipeline.
   - **Significance**: **MINOR** - This is defensive code that exists but doesn't participate in the smell. The comment shows awareness that not everything should use the registry.
   - **Degradation**: None directly, but it highlights the confusion created by having multiple approaches to the same problem.

## Overall Smell Pattern

The **shotgun surgery** smell manifests through the introduction of a global registry pattern that forces participation from 6+ unrelated modules. The core issue is architectural:

1. **Strategy Pattern Gone Wrong**: The registry uses a strategy pattern, but instead of having strategies injected at a single composition point, it allows scattered registration from anywhere.

2. **Import-Time Side Effects**: Five modules register normalizers at import time, creating hidden coupling and initialization order dependencies.

3. **Violated Principles**:
   - **Single Responsibility**: Each module now has dual responsibilities (its original purpose + registration)
   - **Open/Closed**: Adding a new normalization rule requires modifying multiple files
   - **Dependency Inversion**: Low-level modules (compat, utils) depend on high-level coordination (globals)
   - **Cohesion**: Related functionality (all the normalizers) is scattered across unrelated modules

4. **Future Maintenance Impact**: Any change to the normalization pipeline requires understanding and potentially modifying 7+ files. Adding a new normalizer means choosing which module to pollute with the registration call.

## Severity Ranking (Most to Least Important)

1. **`globals.py` registry mechanism** - The root cause that enables all other problems
2. **Module-level registrations in `_compat.py`, `core.py`, `decorators.py`, `types.py`, `utils.py`** - These five are equally critical as they demonstrate the scattered responsibility
3. **`termui.py` application point** - Important because it's the coordination point that reveals the implicit dependencies
4. **`exceptions.py` parallel registry** - Shows the design problem spreading
5. **`testing.py` comment** - Minor, just documentation of the confusion

The root cause is clearly the registry pattern in `globals.py`. The five registration sites are equally problematic symptoms.

## What Was Degraded Overall

1. **Module Cohesion**: Six modules lost their focused purpose and now participate in input normalization
2. **Coupling**: Every module that registers a normalizer is now coupled to `globals.py` and implicitly to `termui.py`
3. **Testability**: You can't test normalizers in isolation without understanding the global registration order
4. **Discoverability**: Finding all normalizers requires searching across the entire codebase
5. **Initialization Predictability**: Module import order now matters because of side effects
6. **Change Impact**: A simple feature (input normalization) now touches 7+ files
7. **Code Locality**: Related logic (the five normalizer functions) is scattered with no single source of truth

## Key Evaluation Signals

A proper fix should be evaluated on:

1. **Consolidation**: Are all normalizer functions and their coordination logic moved to a single module or closely related modules? The smell is fixed when you can understand the entire normalization pipeline by looking at 1-2 files, not 7.

2. **Registration Elimination**: Does the fix eliminate the global registry pattern and module-level registration side effects? Look for removal of `register_input_normalizer` calls from `_compat.py`, `core.py`, `decorators.py`, `types.py`, and `utils.py`.

3. **Import Graph Simplification**: Does the fix reduce cross-module dependencies? Specifically, modules like `decorators.py` and `utils.py` should not need to import from `globals.py` for this feature.

4. **Explicit Composition**: Is the pipeline constructed explicitly in one place (likely in `termui.py` or a dedicated normalization module) rather than assembled implicitly through scattered registrations?

5. **Cohesion Restoration**: Do modules return to their original purposes? `_compat.py` should only handle compatibility, `decorators.py` should only handle decorators, etc.

**What distinguishes thorough from superficial fixes:**
- **Superficial**: Moving all normalizers to one file but keeping the registry pattern
- **Thorough**: Eliminating the registry entirely and composing normalizers explicitly at the point of use
- **Gold standard**: Creating a dedicated normalization module that owns all the logic, with `termui.py` importing a single high-level function

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

### Context for "shotgun_surgery"

**Focus:** Whether scattered logic is properly consolidated into a single location so that a conceptual change requires modifying only one place.
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
