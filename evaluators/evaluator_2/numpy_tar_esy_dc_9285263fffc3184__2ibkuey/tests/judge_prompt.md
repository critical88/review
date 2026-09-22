You are an expert code reviewer evaluating a refactored version of code that originally contained a "data_clumps" code smell.

## Context
- **Smell Type**: data_clumps
- **Smell Description**: Groups of variables that are frequently passed together, suggesting poor encapsulation or missing class abstraction.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Changes Analysis

### 1. Introduction of `_normalize_format_args()` function (lines 47-58)

**What it does**: This function takes five parameters (`formats`, `names`, `titles`, `aligned`, `byteorder`) and performs minimal normalization on them. The logic is largely trivial - most operations are no-ops (like `if aligned is None: aligned = False` or `if byteorder is None: byteorder = None`). It returns all five parameters as a tuple.

**Significance**: **Moderate** - This function is the first symptom of the data clump smell. It creates a wrapper that accepts the clumped parameters but does nothing meaningful to justify their grouping. The function itself adds no real value beyond type conversions.

**What it degrades**: 
- **Cohesion**: The function lacks clear purpose - it's doing unrelated transformations on unrelated data
- **Readability**: The trivial operations (especially `byteorder = None` when it's already None) make the code confusing
- **API surface**: Adds an unnecessary internal API point

### 2. Introduction of `_build_descr_from_format_args()` function (lines 61-65)

**What it does**: This function acts as a thin wrapper that takes the same five parameters, calls `_normalize_format_args()`, unpacks the results, and then passes them to `format_parser().dtype`.

**Significance**: **CRITICAL** - This is the core manifestation of the data clump smell. The function exists solely to shuttle the five-parameter tuple around. It's a "middle-man" function with no real logic beyond parameter forwarding. The fact that this wrapper was created indicates the developers recognized these five parameters travel together but chose wrapping over proper abstraction.

**What it degrades**:
- **Design intent**: Obscures what's actually happening (creating a descriptor from format parser)
- **Indirection**: Adds unnecessary layer between caller and `format_parser`
- **Maintainability**: Changes to format specification now require updating multiple wrapper functions
- **Code smell amplification**: Makes the data clump "official" by institutionalizing it in helper functions

### 3. Six call-site replacements (lines 415, 657, 742, 838, 936, 1062)

**What it does**: Each location replaces a direct call to `format_parser(formats, names, titles, aligned, byteorder).dtype` with a call to `_build_descr_from_format_args(formats, names, titles, aligned, byteorder)`.

**Significance**: **CRITICAL** - These changes demonstrate the pervasiveness of the data clump. The same five parameters appear together at six different locations in the codebase, always in the same order, always passed together. This repetition is the defining characteristic of the data clump smell.

**What it degrades**:
- **Coupling**: All six call sites are now coupled to the wrapper function instead of directly to `format_parser`
- **Flexibility**: Cannot easily vary which parameters are passed at different call sites
- **Discovery**: The relationship between these parameters and `format_parser` is now hidden behind indirection
- **Duplication**: The parameter list `formats, names, titles, aligned, byteorder` is still duplicated 6+ times, just with a different function name

### 4. Submodule pointer updates (highway and meson)

**What it does**: Updates Git submodule references.

**Significance**: **Minor/None** - These are unrelated infrastructure changes and don't contribute to the data clump smell.

**What it degrades**: Nothing related to this smell.

## Overall Smell Pattern

The data clump smell manifests through the **repeated co-occurrence of five parameters** (`formats`, `names`, `titles`, `aligned`, `byteorder`) that are always passed together as a group. This violates the **"Replace Data Value with Object"** principle and the **Single Responsibility Principle**.

The smell indicates a **missing abstraction** - these five parameters clearly represent a cohesive concept (format specification configuration), but instead of creating a proper class or dataclass to encapsulate them, the code:
1. Passes them as individual parameters everywhere
2. Creates wrapper functions that shuttle them around as a group
3. Maintains this fragile coupling across multiple call sites

The wrapper functions (`_normalize_format_args` and `_build_descr_from_format_args`) actually **worsen** the smell because they:
- Acknowledge that these parameters belong together (by always handling them as a unit)
- But fail to properly encapsulate them into a proper abstraction
- Add indirection without adding value
- Create the illusion of organization while maintaining the underlying structural problem

## Severity Ranking (Most to Least Important)

1. **CRITICAL**: The six call-site replacements - These demonstrate the pervasive nature of the clump and the actual problem scope
2. **CRITICAL**: `_build_descr_from_format_args()` function - This institutionalizes the smell by making it "official"
3. **MODERATE**: `_normalize_format_args()` function - Supporting function that enables the wrapper pattern
4. **MINOR/NONE**: Submodule updates - Unrelated changes

The root cause is the **absence of a proper FormatSpecification class/object** that would encapsulate these five parameters. The wrapper functions are symptoms, not causes - they're a failed attempt to manage complexity without proper abstraction.

## What Was Degraded Overall

**Concrete Quality Degradations:**

1. **Cohesion**: The format specification concept is scattered across five individual parameters instead of being unified in a single object

2. **Coupling**: Six different call sites are now coupled to:
   - The specific parameter order
   - The wrapper function signature
   - The normalization function
   - Each other (through shared convention)

3. **Maintainability**: 
   - Adding a new format parameter requires updating 8+ locations (wrapper functions + all call sites)
   - Cannot independently evolve different usages
   - Difficult to provide defaults or validation logic

4. **Readability**:
   - The intent (creating a format specification) is obscured by parameter lists
   - Wrapper functions add cognitive load without clarity
   - No self-documenting structure

5. **Type Safety**:
   - No compile-time guarantee that all five parameters are provided correctly
   - Easy to swap parameter positions (titles vs names)
   - Cannot enforce invariants between parameters

6. **Testability**:
   - Testing format specifications requires providing all five parameters
   - Cannot mock or substitute the format specification concept
   - Harder to create test fixtures

## Key Evaluation Signals

When evaluating a fix for this smell, the most important signals are:

### PRIMARY (Must-have):

1. **Elimination of repeated parameter groups**: The fix should replace the five-parameter list with a single cohesive object/class that encapsulates format specifications

2. **Removal of wrapper functions**: Both `_normalize_format_args` and `_build_descr_from_format_args` should be removed or fundamentally changed, as they exist only to manage the clump

3. **Unified abstraction**: A proper `FormatSpecification` or similar class should be introduced that:
   - Encapsulates all five parameters
   - Provides validation/normalization in its constructor
   - Has a clear interface for conversion to dtype

4. **Call-site simplification**: All six call sites should pass a single format specification object instead of five individual parameters

### SECONDARY (Distinguishes excellent from acceptable):

5. **Backward compatibility handling**: How the fix manages the existing public API (functions like `fromarrays`, `fromrecords`, etc. that accept these parameters)

6. **Parameter defaults and validation**: Whether the fix moves default value logic and validation into the new abstraction

7. **Related parameter reduction**: Whether the fix addresses the broader pattern in function signatures (e.g., `fromarrays` takes 10+ parameters, many related)

### SUPERFICIAL FIXES TO AVOID:

- Simply renaming the wrapper functions without eliminating them
- Creating a dictionary or tuple to hold parameters (still not a proper abstraction)
- Only fixing some call sites while leaving others with the parameter list
- Adding more wrapper functions or indirection layers

The **gold standard** would be introducing a proper `FormatSpecification` class that these functions accept instead of five individual parameters, with the class handling all normalization internally.

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
