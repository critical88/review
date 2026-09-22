You are an expert code reviewer evaluating a refactored version of code that originally contained a "god_classes" code smell.

## Context
- **Smell Type**: god_classes
- **Smell Description**: A class that centralizes too much functionality, violating single responsibility and becoming hard to maintain.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Change Analysis

### 1. Addition of `_shared_platform_state` global dictionary and `_populate_platform_state()` function (cpuinfo.py)

**What it does**: Creates a module-level global dictionary that captures platform information (CPU count, 64-bit status, platform type, machine type) and populates it at module import time.

**Significance**: **Moderate** - This establishes a global state mechanism that violates encapsulation. The data is collected from the existing `cpu` object and platform modules, then stored in a globally accessible dictionary.

**What it degrades**: 
- **Encapsulation**: Platform information that was previously accessed through proper API calls is now duplicated into global state
- **Coupling**: Creates a new coupling point between cpuinfo.py and any module importing this state
- **Testability**: Global state initialized at import time is harder to mock/control in tests

### 2. Import and copying of `_shared_platform_state` into `self._compiler_state` (system_info.py, __init__ method)

**What it does**: In the `system_info` class's `__init__` method, imports the global platform state dictionary and creates a copy as an instance variable `_compiler_state`.

**Significance**: **Critical** - This is the core of the god class smell. The `system_info` class now absorbs platform detection responsibilities that don't belong to it. This class is supposed to gather information about external libraries (BLAS, LAPACK, etc.), not manage platform state.

**What it degrades**:
- **Single Responsibility Principle**: The `system_info` class now has an additional responsibility (platform state management) beyond its original purpose
- **Cohesion**: Platform state management is orthogonal to library detection/configuration
- **Class bloat**: Adds state that must be maintained across all instances

### 3. Usage of `self._compiler_state` instead of direct `sys.platform` calls (library_extensions method)

**What it does**: Replaces three direct `sys.platform` checks with lookups from `self._compiler_state.get('platform', sys.platform)`.

**Significance**: **Moderate** - This change demonstrates how the platform state is being used. It adds indirection without clear benefit, since `sys.platform` is globally accessible anyway.

**What it degrades**:
- **Readability**: Less clear what's happening compared to direct `sys.platform` access
- **Indirection**: Adds unnecessary layer between code and the information it needs
- **Maintainability**: Developers now need to track where `_compiler_state` comes from and why it's used

### 4. Addition of `_resolve_msvc_gfortran_libs()` method to `system_info` class

**What it does**: Extracts the implementation from `openblas_info.check_msvc_gfortran_libs()` into a new method on the base `system_info` class. This method handles creation of virtual static library files for MSVC/gfortran compatibility.

**Significance**: **Critical** - This is a major contributor to the god class smell. The method is specific to MSVC/gfortran library resolution but is placed in the base class, making it available to all subclasses regardless of whether they need it.

**What it degrades**:
- **API surface**: Base class now has 73 lines of highly specialized code that most subclasses don't need
- **Cohesion**: MSVC/gfortran-specific logic doesn't belong in the general-purpose base class
- **Class complexity**: Adds significant complexity to an already complex class
- **Single Responsibility**: The base class now handles compiler-specific library resolution strategies

### 5. Addition of `_verify_link_symbols()` method to `system_info` class

**What it does**: Extracts symbol verification logic into a new base class method. This method creates test C code, compiles it, links it, and verifies that specified symbols are available.

**Significance**: **Critical** - Another major god class contributor. This is a general utility function for link testing that's now embedded in the base class, adding 40+ lines of complex compilation/linking logic.

**What it degrades**:
- **API surface**: Base class grows with specialized testing functionality
- **Cohesion**: Symbol verification is a utility function, not core state/configuration logic
- **Separation of Concerns**: Mixing build/test logic with configuration data gathering
- **Complexity**: Adds file I/O, temporary directory management, compilation, and exception handling to the class

### 6. Refactoring of `openblas_info.check_msvc_gfortran_libs()` to delegate

**What it does**: Replaces the original 67-line implementation with a one-line delegation to `self._resolve_msvc_gfortran_libs()`.

**Significance**: **Minor** - This is a consequence of change #4. While it reduces duplication in this specific method, it does so by centralizing the logic inappropriately.

**What it degrades**: Nothing directly; this is actually cleaner locally but contributes to the overall god class problem.

### 7. Refactoring of `openblas_info.check_symbols()` to delegate

**What it does**: Replaces the original 40-line implementation with a three-line delegation to `self._verify_link_symbols()`.

**Significance**: **Minor** - Similar to #6, this is a consequence of change #5.

**What it degrades**: Nothing directly; same trade-off as #6.

### 8. Refactoring of `flame_info.check_embedded_lapack()` to delegate

**What it does**: Replaces the original 30-line implementation with a one-line delegation to `self._verify_link_symbols()`.

**Significance**: **Minor** - Another consequence of change #5.

**What it degrades**: Nothing directly; same pattern as #6 and #7.

### 9. Submodule version updates (highway, meson)

**What it does**: Updates git submodule commit hashes for vendored dependencies.

**Significance**: **Irrelevant** - These changes are unrelated to the god class smell and appear to be incidental to the commit.

**What it degrades**: Nothing related to the smell.

### 10. Variable renaming (`l` to `lib_names`)

**What it does**: Renames a poorly-named variable from `l` to the more descriptive `lib_names`.

**Significance**: **Irrelevant** - This is a positive readability improvement unrelated to the god class smell.

**What it degrades**: Nothing; this is an improvement.

## Overall Smell Pattern

The changes work together to create a god class by:

1. **Centralizing platform state management**: The `system_info` base class absorbs platform detection responsibilities through `_compiler_state`
2. **Centralizing specialized compilation utilities**: Two complex, specialized methods (`_resolve_msvc_gfortran_libs` and `_verify_link_symbols`) are pulled into the base class
3. **Expanding responsibilities**: The class that should focus on library information gathering now also handles platform state, compiler-specific workarounds, and link testing

**Design principles violated**:
- **Single Responsibility Principle**: The `system_info` class now has at least three distinct responsibilities (library info gathering, platform state management, compilation testing)
- **Interface Segregation**: Subclasses inherit methods they may not need (many won't need MSVC/gfortran resolution)
- **Separation of Concerns**: Platform detection, library configuration, and build testing are mixed together
- **Proper Abstraction Levels**: The base class contains both high-level configuration logic and low-level file I/O/compilation details

## Severity Ranking (Most to Least Important)

1. **Addition of `_verify_link_symbols()` to system_info** (Change #5) - CRITICAL ROOT CAUSE
   - Adds 40+ lines of complex, low-level logic to base class
   - Mixes build/test concerns with configuration gathering
   - Used by multiple subclasses but doesn't belong in base

2. **Addition of `_resolve_msvc_gfortran_libs()` to system_info** (Change #4) - CRITICAL ROOT CAUSE
   - Adds 70+ lines of highly specialized logic to base class
   - Compiler-specific workaround polluting general-purpose class
   - Only relevant to OpenBLAS on MSVC, yet in base class

3. **Addition of `_compiler_state` instance variable** (Change #2) - CRITICAL ROOT CAUSE
   - Fundamentally changes class responsibility
   - Adds platform management to library info class
   - Creates unnecessary state duplication

4. **Usage of `_compiler_state` over `sys.platform`** (Change #3) - MODERATE SUPPORTING
   - Demonstrates the platform state is actually being used
   - Adds indirection without clear benefit
   - Makes the design choice concrete

5. **Addition of `_shared_platform_state` global** (Change #1) - MODERATE SUPPORTING
   - Enables the cross-module state sharing
   - Creates global state anti-pattern
   - But mostly a dependency for change #2

6. **Delegation changes in subclasses** (Changes #6, #7, #8) - MINOR CONSEQUENCES
   - These are results of the base class bloat
   - Locally cleaner but symptom of the root problem

7. **Submodule and variable rename changes** (Changes #9, #10) - IRRELEVANT NOISE

## What Was Degraded Overall

**Cohesion**: The `system_info` class loses focus. It was designed to gather library information but now also manages platform state and provides compilation testing utilities. These responsibilities are loosely related at best.

**Coupling**: New coupling is introduced between cpuinfo.py and system_info.py through the global state dictionary. The base class also becomes more tightly coupled to specific build scenarios (MSVC/gfortran).

**API Surface**: The base class grows from being a focused library information gatherer to a 113-line method bloat (73 + 40 lines of new methods), exposing internal utility functions that should be private helpers or separate utilities.

**Maintainability**: 
- Changes to platform detection now affect both cpuinfo and system_info
- The base class is harder to understand due to mixed concerns
- Testing becomes more complex due to the entangled responsibilities

**Extensibility**: Subclasses inherit unnecessary methods and state. New subclasses get platform state and compilation utilities whether they need them or not.

**Single Responsibility**: This is the core degradation. The `system_info` class violates SRP by taking on platform management and build testing roles.

## Key Evaluation Signals

When evaluating a fix for this god class smell, look for:

### 1. **Responsibility Separation** (MOST CRITICAL)
- **Excellent fix**: Platform state management removed entirely from `system_info`; compilation/link testing utilities extracted to separate helper class or module
- **Poor fix**: Methods remain in `system_info` base class, just renamed or reorganized

### 2. **Base Class Simplification** (CRITICAL)
- **Excellent fix**: `_resolve_msvc_gfortran_libs()` and `_verify_link_symbols()` are NOT methods of `system_info` base class; they exist as standalone functions, helper classes, or mixin classes used only where needed
- **Poor fix**: Methods stay in base class but marked "internal" or moved to end of class

### 3. **State Management Clarity** (CRITICAL)
- **Excellent fix**: No `_compiler_state` instance variable; platform checks use direct `sys.platform` or a dedicated platform info object; no global `_shared_platform_state` dictionary
- **Poor fix**: State renamed but still duplicated in `system_info` instances

### 4. **Subclass Interface Cleanliness** (IMPORTANT)
- **Excellent fix**: Subclasses like `flame_info` and `openblas_info` only inherit library-discovery-related methods; specialized utilities accessed through composition or explicit imports
- **Poor fix**: All subclasses still inherit the bloated interface

### 5. **Method Cohesion** (IMPORTANT)
- **Excellent fix**: All methods in `system_info` relate to library information gathering/configuration; compilation testing in separate module/class; MSVC-specific logic only in MSVC-specific contexts
- **Poor fix**: Mixed concerns remain bundled together

**The litmus test**: Can you describe what the `system_info` class does in one sentence without using "and"? A proper fix should enable: "The system_info class gathers information about installed libraries for build configuration." Currently, you'd need: "The system_info class gathers library information AND manages platform state AND provides compilation testing utilities AND handles MSVC/gfortran compatibility."

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
