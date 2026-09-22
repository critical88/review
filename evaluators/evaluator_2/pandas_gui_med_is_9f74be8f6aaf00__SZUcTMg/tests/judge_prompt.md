You are an expert code reviewer evaluating a refactored version of code that originally contained a "interface_segregation" code smell.

## Context
- **Smell Type**: interface_segregation
- **Smell Description**: When interfaces are too large or force implementing classes to depend on methods they don't use, violating interface segregation principle.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

# Detailed Analysis of Interface Segregation Smell

## Individual Change Analysis

### 1. Addition of `_dtype_affinity` property to ExtensionDtype (base.py, lines 448-465)
**What it does**: Adds a computed property that returns a string categorizing dtypes into affinity groups ("numeric", "temporal", "boolean", "textual", "structured", "object").

**Significance**: CRITICAL - This is a core component of the interface bloat. It forces all ExtensionDtype subclasses to inherit a property that many won't need or use.

**Degradation**: 
- **Interface bloat**: Adds an abstract concept that 90%+ of ExtensionDtype implementations won't utilize
- **Coupling**: Introduces a new classification system that clients must understand even if they don't use it
- **API surface**: Increases the public API footprint of the base class unnecessarily

### 2. Addition of `_validate_cast_integrity()` method to ExtensionDtype (base.py, lines 467-492)
**What it does**: Adds a 26-line method to validate whether casting from this dtype to a target dtype is safe, with logic for comparing affinity groups and numpy dtype kinds.

**Significance**: CRITICAL - This is the most egregious violation. A complex method with specific casting logic is pushed into the base interface.

**Degradation**:
- **Forced dependencies**: All ExtensionDtype subclasses must now carry this method even if they have no casting requirements or use completely different casting semantics
- **Single Responsibility Principle**: Mixes type promotion/casting concerns into the dtype definition interface
- **Cohesion**: Reduces cohesion by bundling unrelated concerns (dtype representation + casting validation)
- **Maintenance burden**: Every dtype implementation must understand and potentially override this complex logic

### 3. Addition of `_get_cast_promotion_rules()` method to ExtensionDtype (base.py, lines 494-515)
**What it does**: Returns a dictionary with 4 keys describing promotion behavior: "can_upcast", "affinity", "preserves_na", "promotion_priority".

**Significance**: CRITICAL - Another complex method added to the base interface that forces all subclasses to support a promotion rules dictionary format.

**Degradation**:
- **Interface segregation violation**: Forces all ExtensionDtype implementations to support a promotion rules API even if they never participate in type promotion
- **Magic numbers/strings**: Introduces a dictionary-based protocol that's not type-safe and requires documentation
- **Coupling to promotion system**: Tightly couples the dtype interface to the internal type promotion machinery

### 4. Override of `_dtype_affinity` in PandasExtensionDtype (dtypes.py, lines 148-155)
**What it does**: Provides a more specific implementation based on numpy kind codes.

**Significance**: MODERATE - This is a consequence of the base class change, showing that subclasses must now implement/override this property.

**Degradation**: Demonstrates the cascading effect where intermediate base classes must deal with the bloated interface.

### 5. Override of `_validate_cast_integrity()` in DatetimeTZDtype (dtypes.py, lines 761-768)
**What it does**: Adds timezone-aware cast validation logic for datetime types.

**Significance**: MODERATE - Shows specialized casting logic that should be encapsulated separately, not forced into every dtype.

**Degradation**: Forces DatetimeTZDtype to understand and override complex casting logic that's orthogonal to its primary purpose (representing a timezone-aware datetime type).

### 6. Override of `_get_cast_promotion_rules()` in DatetimeTZDtype (dtypes.py, lines 770-773)
**What it does**: Customizes promotion rules by setting can_upcast=True and priority=30.

**Significance**: MODERATE - Demonstrates that specific dtypes need custom promotion behavior, but are forced to express it through the universal interface.

**Degradation**: Each dtype that participates in promotion must now understand the promotion rules dictionary format and priority system.

### 7. Multiple property/method overrides in BaseMaskedDtype (dtypes.py, lines 1632-1652)
**What it does**: Adds `_dtype_affinity` property, `_validate_cast_integrity()` and `_get_cast_promotion_rules()` methods to masked array dtypes.

**Significance**: MODERATE - Shows the interface burden propagating to yet another dtype family.

**Degradation**: BaseMaskedDtype, which should focus on nullable/masked array semantics, now must also manage casting and promotion concerns.

### 8. Override of `_get_cast_promotion_rules()` in NumericDtype (numeric.py, lines 62-66)
**What it does**: Customizes promotion priority based on itemsize (10 + itemsize).

**Significance**: MODERATE - Shows that numeric types need promotion rules but must use the bloated base interface.

**Degradation**: NumericDtype must understand the promotion system even though this is really about type resolution, not numeric representation.

### 9. Override of `_validate_cast_integrity()` in SparseDtype (dtypes.py, lines 2091-2094)
**What it does**: Validates casts between sparse dtypes by comparing underlying dtype kinds.

**Significance**: MINOR - Another implementation forced to deal with the interface, though sparse arrays have unique casting semantics.

**Degradation**: SparseDtype must implement casting validation using an interface designed for dense dtypes.

### 10. New function `_pre_validate_type_promotion()` in cast.py (lines 1429-1467)
**What it does**: A 39-line function that uses the new dtype methods to attempt early type promotion resolution based on affinity and promotion rules.

**Significance**: MODERATE - This is the consumer code that justifies the interface bloat, showing what "benefits" the smell supposedly provides.

**Degradation**:
- **Coupling**: Creates tight coupling between the casting system and every ExtensionDtype
- **Complexity**: Adds another layer of complexity to type resolution
- **Questionable value**: The function does extensive validation but always returns None, suggesting the interface burden doesn't provide real value

### 11. Integration in `find_common_type()` (cast.py, lines 1516-1519)
**What it does**: Calls `_pre_validate_type_promotion()` before the existing type resolution logic.

**Significance**: MINOR - Simple integration point, but shows the unused result (promoted is always None).

**Degradation**: Adds overhead to type resolution without clear benefit.

## Overall Smell Pattern

**Core Violation**: The Interface Segregation Principle (ISP) states that "clients should not be forced to depend on interfaces they do not use." This diff violates ISP by:

1. **Fat Interface**: Adding three complex members (`_dtype_affinity`, `_validate_cast_integrity()`, `_get_cast_promotion_rules()`) to the base ExtensionDtype class
2. **Universal Requirements**: Forcing ALL dtype implementations to support type promotion and casting validation, even when they don't participate in these operations
3. **Feature Coupling**: Bundling casting/promotion concerns (which only matter during specific operations) into the fundamental dtype interface (which all types must implement)

**How they work together**: The base interface additions (changes 1-3) create the bloated interface. The subclass overrides (changes 4-9) demonstrate how this burden propagates through the hierarchy. The consumer code (changes 10-11) shows the supposed justification, but its ineffectiveness (always returning None) suggests the interface bloat provides little actual value.

**Design Principle Violated**: 
- **Interface Segregation Principle (primary)**: Not all ExtensionDtype implementations need casting validation or promotion rules
- **Single Responsibility Principle**: Dtypes should represent types, not also manage casting and promotion logic
- **Separation of Concerns**: Type representation is mixed with type resolution/promotion concerns

## Severity Ranking (Most to Least Important)

1. **CRITICAL - `_validate_cast_integrity()` addition (change 2)**: The most complex method (26 lines) forced into the base interface, with intricate logic that most implementations don't need

2. **CRITICAL - `_get_cast_promotion_rules()` addition (change 3)**: Forces all dtypes to support a dictionary-based promotion protocol, tightly coupling them to the promotion system

3. **CRITICAL - `_dtype_affinity` property addition (change 1)**: Creates a classification system that all dtypes must support but few actually need

4. **MODERATE - `_pre_validate_type_promotion()` function (change 10)**: The consumer justifying the interface bloat, but its ineffectiveness reveals the smell

5. **MODERATE - All subclass overrides (changes 4-9)**: These demonstrate the cascade effect but are consequences, not root causes

6. **MINOR - Integration in find_common_type (change 11)**: Simple glue code, symptom not cause

## What Was Degraded Overall

**Concrete impacts**:

1. **Interface Complexity**: ExtensionDtype interface grew from ~15 members to ~18+ members, a 20% increase in API surface

2. **Coupling**: Every ExtensionDtype implementation is now coupled to:
   - The affinity classification system
   - The promotion rules dictionary format
   - The cast validation logic
   - The priority-based promotion resolution system

3. **Cohesion**: ExtensionDtype now has mixed responsibilities:
   - Defining type characteristics (original purpose)
   - Validating type casts (new)
   - Providing promotion rules (new)
   - Categorizing type affinity (new)

4. **Maintainability**: 
   - New dtype implementations must understand 3 additional complex methods
   - Changes to promotion logic require touching the base dtype interface
   - Testing burden increased: every dtype must test casting/promotion even if irrelevant

5. **Flexibility**: Dtypes that don't participate in promotion (e.g., categorical, sparse with special semantics) still carry the interface burden

6. **Documentation Burden**: The base interface now requires explaining affinity groups, promotion priorities, and cast validation to all dtype implementers

7. **Type Safety**: The dictionary-based promotion rules are not type-safe and rely on string keys

## Key Evaluation Signals

**What distinguishes a thorough fix from superficial:**

1. **Interface Segregation**: 
   - EXCELLENT: The three methods (`_dtype_affinity`, `_validate_cast_integrity`, `_get_cast_promotion_rules`) are removed from ExtensionDtype entirely or made opt-in
   - POOR: Methods remain in base class but with different implementations

2. **Separation of Concerns**:
   - EXCELLENT: Casting/promotion logic moved to separate interfaces/protocols that dtypes can optionally implement
   - GOOD: Logic moved to separate mixin classes
   - POOR: Logic remains bundled in base dtype interface

3. **Client Specificity**:
   - EXCELLENT: Only dtypes that actually participate in type promotion implement promotion-related interfaces
   - GOOD: Base class provides no-op defaults, specific dtypes override
   - POOR: All dtypes forced to implement promotion methods

4. **Coupling Reduction**:
   - EXCELLENT: Type promotion system uses duck-typing or protocol checking instead of requiring base class methods
   - GOOD: Promotion system queries dtypes through a separate adapter/visitor
   - POOR: Direct method calls on dtype objects remain

5. **API Surface**:
   - EXCELLENT: Base ExtensionDtype returns to ~15 members focused on type representation
   - ACCEPTABLE: Additional members remain but documented as optional/advanced
   - POOR: No reduction in base interface size

6. **Practical Usage**:
   - EXCELLENT: Dtypes like CategoricalDtype, SparseDtype don't implement promotion methods at all
   - GOOD: Simple dtypes provide minimal stub implementations
   - POOR: All dtypes forced to provide full implementations

**Most Important Signal**: Did the fix move casting/promotion concerns OUT of the base ExtensionDtype interface? This is the core of the smell - if these methods remain required in the base interface, the smell persists regardless of other improvements.

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
