You are an expert code reviewer evaluating a refactored version of code that originally contained a "data_clumps" code smell.

## Context
- **Smell Type**: data_clumps
- **Smell Description**: Groups of variables that are frequently passed together, suggesting poor encapsulation or missing class abstraction.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

This diff introduces a classic "data_clumps" code smell by creating utility functions that pass around groups of related parameters that appear together repeatedly. Let me analyze each change:

## Individual Changes

### 1. Addition of `prepare_scoring_labels()` function in utils.py
**What it does**: Extracts logic that conditionally maps encoded target labels back to original class labels. Takes three parameters: `targets`, `apply_class_mapping` (boolean), and `class_labels` (array or None).

**Significance**: **Critical** - This is a primary manifestation of the smell. The function signature reveals a data clump: `apply_class_mapping` and `class_labels` are always passed together and their relationship is implicit (the boolean controls whether to use the array).

**What it degrades**: 
- **Cohesion**: These two parameters form a conceptual unit but are kept separate
- **API clarity**: The boolean flag pattern with optional data is a classic anti-pattern
- **Type safety**: `class_labels` being None when `apply_class_mapping` is False creates conditional logic based on parameter combinations

### 2. Addition of `evaluate_split_score()` function in utils.py
**What it does**: Wraps scorer invocation with conditional sample weight handling. Takes five parameters: `scorer`, `estimator`, `split_features`, `split_targets`, and `split_weights`.

**Significance**: **Critical** - Another primary smell instance. The parameter group `(split_features, split_targets, split_weights)` represents a data clump - these three always travel together and represent "a data split."

**What it degrades**:
- **Abstraction**: Missing a proper "DataSplit" or "ScoringContext" object that would encapsulate these related values
- **Coupling**: Forces callers to always gather and pass these three pieces of data together
- **Readability**: The function signature is verbose with repetitive "split_" prefixes hinting at the missing abstraction

### 3. Addition of imports in gradient_boosting.py
**What it does**: Imports the two new utility functions.

**Significance**: **Minor** - Supporting change that enables the smell but isn't the root cause.

**What it degrades**: Module coupling (now depends on these utility functions).

### 4. Addition of `_SMALL_TRAINSET_SUBSAMPLE_SIZE` constant
**What it does**: Extracts magic number 10_000 to a module-level constant.

**Significance**: **Minor/Irrelevant** - This is actually a good practice (extracting magic numbers). Not related to the data clumps smell.

**What it degrades**: Nothing; this is a minor improvement.

### 5. Two call sites adding `apply_class_mapping` and `class_labels` parameters (lines 853-854, 1032-1033)
**What it does**: At two locations where `_check_early_stopping_scorer()` is called, adds two new keyword arguments that compute the same values: `is_classifier(self)` and `getattr(self, "classes_", None)`.

**Significance**: **Critical** - This is the smoking gun that reveals the data clump. The **exact same pair of expressions** appears in multiple locations, passed together to the same function. This repetition is the hallmark of data clumps.

**What it degrades**:
- **DRY principle**: Identical logic duplicated across call sites
- **Maintainability**: Changes to how classification status is determined must be replicated
- **Code clarity**: The relationship between these parameters is implicit at every call site

### 6. Modified `_check_early_stopping_scorer()` signature and body
**What it does**: Adds two parameters (`apply_class_mapping`, `class_labels`) to the method signature and replaces inline `if is_classifier(self):` checks with calls to `prepare_scoring_labels()`.

**Significance**: **Critical** - This change propagates the data clump through the class interface. The method now receives data that could be derived or encapsulated better.

**What it degrades**:
- **Method signature complexity**: Goes from 6 to 8 parameters
- **Encapsulation**: The method receives information it could potentially derive from `self`
- **Interface stability**: Adding parameters to an existing method increases coupling with callers

### 7. Replacement of inline label mapping logic (lines 1143-1145, 1158-1160)
**What it does**: Replaces `if is_classifier(self): y = self.classes_[y.astype(int)]` with calls to `prepare_scoring_labels()`.

**Significance**: **Moderate** - Shows the symptom of the smell: extracting simple, clear inline logic into a function that requires passing additional parameters.

**What it degrades**: 
- **Readability**: The original inline code was arguably clearer about what it does
- **Locality**: Logic that directly used `self` now requires passing data as parameters

### 8. Modified `_score_with_raw_predictions()` body (lines 1176-1178)
**What it does**: Replaces a simple if-else that calls `self._scorer()` with or without sample_weight, replacing it with a call to `evaluate_split_score()`.

**Significance**: **Moderate** - Demonstrates over-abstraction: wrapping a 4-line if-else with a function call that requires passing multiple parameters.

**What it degrades**:
- **Simplicity**: The original if-else was straightforward; the wrapper adds indirection
- **Performance**: Adds function call overhead for trivial logic

## Overall Smell Pattern

The core pattern is: **related data items that conceptually belong together are passed as separate parameters through multiple function calls**. Specifically:

1. **Classification context clump**: `(apply_class_mapping, class_labels)` - a boolean and an optional array that are meaningfully related (the boolean controls whether the array is used)

2. **Data split clump**: `(split_features, split_targets, split_weights)` - three arrays that together represent a single logical entity: a scored data split

The design principle violated is **Missing Abstraction / Poor Encapsulation**. When multiple pieces of data travel together repeatedly, they signal the need for a higher-level abstraction (like a `ClassificationContext` or `DataSplit` class). Instead, the code treats them as primitive parameters, forcing every function in the call chain to know about and handle these related pieces.

## Severity Ranking (Most to Least Important)

1. **The two call sites adding parameter pairs** (lines 853-854, 1032-1033) - ROOT CAUSE: This duplication of `is_classifier(self)` and `getattr(self, "classes_", None)` at multiple call sites is the clearest evidence of the smell.

2. **`prepare_scoring_labels()` function signature** - ROOT CAUSE: The `(apply_class_mapping, class_labels)` parameter pair is the primary data clump.

3. **`evaluate_split_score()` function signature** - ROOT CAUSE: The `(split_features, split_targets, split_weights)` parameter triplet is the second data clump.

4. **`_check_early_stopping_scorer()` signature modification** - PROPAGATION: Adds parameters to propagate the data clump through the call chain.

5. **Inline logic replacements** in `_check_early_stopping_scorer()` - SYMPTOM: Shows the consequence of the smell but isn't the root cause.

6. **`_score_with_raw_predictions()` body change** - SYMPTOM: Demonstrates over-abstraction as a consequence.

7. **Import additions** - SUPPORTING: Necessary but not meaningful on their own.

8. **Constant addition** - IRRELEVANT: Good practice, unrelated to the smell.

## What Was Degraded Overall

1. **Cohesion**: Related data that should be encapsulated together remains scattered across parameters
2. **Encapsulation**: Class-level state (`self.classes_`, classification status) is extracted and passed as parameters rather than being used directly
3. **API surface complexity**: Method signatures grow longer without corresponding increase in flexibility
4. **Maintainability**: Changes to how classification context or data splits are handled require modifications at multiple call sites
5. **Readability**: Simple, clear inline logic is replaced with function calls requiring parameter threading
6. **Coupling**: Multiple functions become coupled through shared parameter patterns
7. **Discoverability**: The relationship between related parameters (e.g., `apply_class_mapping` controls `class_labels` usage) is implicit rather than enforced by types

## Key Evaluation Signals

A proper fix should be evaluated on these dimensions:

1. **Parameter reduction**: Do the fixed functions have fewer parameters? The data clumps should be replaced with cohesive objects.

2. **Encapsulation of relationships**: Is the relationship between `apply_class_mapping` and `class_labels` made explicit (e.g., through a class/dataclass that enforces their coupling)?

3. **Call site simplification**: Are the duplicate expressions `is_classifier(self)` and `getattr(self, "classes_", None)` eliminated from call sites?

4. **Abstraction quality**: If new classes/objects are introduced, do they represent meaningful domain concepts (like "ClassificationContext" or "ValidationSplit")?

5. **Self-containment**: Does `_check_early_stopping_scorer()` derive needed information from `self` rather than receiving it as parameters?

6. **Preservation of functionality**: Does the fix maintain the same behavior while improving structure?

A **superficial fix** might just rename parameters or add comments. A **thorough fix** would introduce proper abstractions that encapsulate the related data, eliminate parameter duplication at call sites, and make the relationships between parameters explicit through types.

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
