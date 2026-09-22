You are an expert code reviewer evaluating a refactored version of code that originally contained a "interface_segregation" code smell.

## Context
- **Smell Type**: interface_segregation
- **Smell Description**: When interfaces are too large or force implementing classes to depend on methods they don't use, violating interface segregation principle.

### Expert Analysis of the Smell
The following analysis describes the smell that was introduced, its root cause, and what matters most when evaluating whether a fix truly addresses the problem. Use it to inform your scoring — but judge the refactoring on its own merits.

## Individual Change Analysis

### 1. Addition of `ParamType.validate_constraints()` method (lines 162-192 in types.py)

**What it does**: Adds a new hook method to the base `ParamType` class that is called after type conversion to perform additional validation. It includes logic to walk through registered constraint callbacks stored in `ctx._type_constraints`.

**Significance**: **Critical** - This is a root cause of the interface segregation smell.

**What it degrades**: 
- **Interface bloat**: Adds a method that the vast majority of ParamType subclasses don't need (only 2 of the many types override it)
- **Cohesion**: Mixes validation concerns with type conversion concerns in the base interface
- **Coupling**: Introduces tight coupling to context object internals (`ctx._type_constraints`), creating an implicit contract about context structure
- **API surface**: Expands the interface that all subclasses must understand, even if they don't use it

### 2. Addition of `ParamType.get_diagnostic_info()` method (lines 194-228 in types.py)

**What it does**: Adds a method to collect diagnostic information about the type for error messages and debugging. Returns a dictionary with type metadata.

**Significance**: **Critical** - Another root cause of interface segregation violation.

**What it degrades**:
- **Interface bloat**: Forces all ParamType subclasses to inherit a method many don't need specialized behavior for
- **Responsibility**: Mixes debugging/diagnostic concerns with the core type conversion responsibility
- **API surface**: Adds another method to the public interface that implementers must understand
- **Optional dependency**: Creates an optional feature that's baked into the mandatory interface

### 3. Addition of `ParamType.resolve_value_source()` method (lines 230-258 in types.py)

**What it does**: Determines a human-readable label for where a value came from (stdin, explicit, default, etc.).

**Significance**: **Critical** - Third root cause of the smell.

**What it degrades**:
- **Interface bloat**: Another method forced on all subclasses when only a few types need specialized source resolution
- **Single Responsibility Principle**: Adds UI/presentation concerns to a type conversion interface
- **Cohesion**: Value source tracking is orthogonal to type conversion semantics

### 4. Override of `validate_constraints()` in `Choice` class (lines 495-516 in types.py)

**What it does**: Implements constraint validation for Choice type to ensure values remain within the choice set even after conversion.

**Significance**: **Moderate** - Demonstrates the forced implementation pattern.

**What it degrades**:
- **Maintainability**: Choice type now must implement a method for a rare edge case (programmatic value injection)
- **Complexity**: Duplicates validation logic already present in the `convert()` method
- **Clarity**: Unclear when to use `convert()` vs `validate_constraints()`

### 5. Override of `get_diagnostic_info()` in `Choice` class (lines 518-527 in types.py)

**What it does**: Provides diagnostic information specific to Choice type (choices list, case sensitivity, count).

**Significance**: **Moderate** - Shows forced implementation of diagnostic feature.

**What it degrades**:
- **Optional feature cost**: Forces implementation of debugging infrastructure in production type code
- **Maintenance burden**: Every type subclass now needs to maintain diagnostic metadata

### 6. Override of `get_diagnostic_info()` in `_NumberRangeBase` class (lines 705-716 in types.py)

**What it does**: Extends diagnostic info with range-specific details (min, max, clamp settings).

**Significance**: **Moderate** - Another forced implementation example.

**What it degrades**:
- **Code duplication**: Similar diagnostic pattern repeated across multiple types
- **Coupling**: Each type now coupled to the diagnostic infrastructure

### 7. Override of `get_diagnostic_info()` in `File` class (lines 1018-1027 in types.py)

**What it does**: Adds file-specific diagnostic information (mode, encoding, lazy, atomic flags).

**Significance**: **Moderate** - Continues the pattern.

**What it degrades**: Same as above - forced implementation, duplication, coupling.

### 8. Override of `resolve_value_source()` in `File` class (lines 1029-1042 in types.py)

**What it does**: Provides file-specific source resolution (file_object, file_path, stdin/stdout).

**Significance**: **Moderate** - Shows specialized implementation need.

**What it degrades**:
- **Specialization burden**: File type must now understand and implement value source semantics
- **Mixed concerns**: File opening logic mixed with value provenance tracking

### 9. Override of `get_diagnostic_info()` in `Path` class (lines 1230-1243 in types.py)

**What it does**: Adds path-specific diagnostic information (exists, file_okay, dir_okay, permissions, etc.).

**Significance**: **Moderate** - Pattern repetition.

**What it degrades**: Same issues as other diagnostic overrides.

### 10. Override of `resolve_value_source()` in `Path` class (lines 1245-1258 in types.py)

**What it does**: Provides path-specific source resolution (stdin/stdout, resolved_path, relative_path).

**Significance**: **Moderate** - Specialized implementation.

**What it degrades**: Same as File - mixed concerns and specialization burden.

### 11. Override of `validate_constraints()` in `Path` class (lines 1260-1276 in types.py)

**What it does**: Validates path constraints after conversion, though the implementation is mostly a no-op that defers to the base class.

**Significance**: **Minor** - Demonstrates the absurdity of forced implementation.

**What it degrades**:
- **Code noise**: Empty implementation that exists only to satisfy the interface
- **Documentation burden**: Comment explaining why it's a no-op adds confusion
- **False sense of safety**: Implies post-conversion validation is happening when it's not

### 12. Call to `validate_constraints()` in `Parameter.process_value()` (lines 2434-2438 in core.py)

**What it does**: Invokes the new validation hook after type conversion.

**Significance**: **Critical** - This is what forces the interface segregation issue to matter.

**What it degrades**:
- **Implicit contract**: Assumes all types support constraint validation
- **Processing complexity**: Adds another processing step to the value pipeline
- **Debugging difficulty**: Another location where value transformation can occur

### 13. Call to `resolve_value_source()` and metadata storage (lines 2571-2589 in core.py)

**What it does**: Calls `resolve_value_source()` during deprecation warnings and stores the result in context metadata.

**Significance**: **Moderate** - Creates coupling between deprecation logic and type interface.

**What it degrades**:
- **Feature coupling**: Deprecation warnings now depend on type interface methods
- **Context pollution**: Adds metadata to context (`_deprecated_sources`) that's specific to one feature
- **Side effects**: Mutation of context during deprecation processing

## Overall Smell Pattern

This is a textbook **Interface Segregation Principle** violation. The `ParamType` base class has been inflated with three new methods (`validate_constraints`, `get_diagnostic_info`, `resolve_value_source`) that serve distinct purposes:

1. **Post-conversion validation** - only needed by types with complex constraints (Choice, potentially Path)
2. **Diagnostic metadata** - a debugging/error reporting feature that most types don't need specialized handling for
3. **Value source resolution** - a UI/presentation concern relevant mainly to deprecation warnings

The problem is that these optional, specialized concerns are baked into the **mandatory base interface** that all ParamType subclasses must inherit. This violates ISP because:

- **Fat interface**: Clients (subclasses) are forced to depend on methods they don't use
- **Mixed abstraction levels**: The interface combines core type conversion (essential) with debugging, validation, and UI concerns (optional/specialized)
- **False uniformity**: The interface pretends all types need these features equally, when in reality only a few types have specialized needs

The smell manifests as:
- Multiple subclasses implementing these methods with trivial or no-op behavior
- The base implementations doing generic work that doesn't add value for most types
- Coupling between unrelated features (deprecation warnings ↔ type value source resolution)

## Severity Ranking (Most to Least Important)

1. **Adding three new methods to ParamType base class** (validate_constraints, get_diagnostic_info, resolve_value_source) - These are the root cause, creating the fat interface
   
2. **Invocation of validate_constraints in Parameter.process_value()** - This forces the interface to matter by making it part of the processing pipeline

3. **Invocation of resolve_value_source in deprecation logic** - Creates feature coupling that makes the interface dependency concrete

4. **Multiple overrides of get_diagnostic_info** (Choice, _NumberRangeBase, File, Path) - Demonstrates the forced implementation burden across the codebase

5. **Multiple overrides of resolve_value_source** (File, Path) - Shows specialized implementations being forced into the type hierarchy

6. **Multiple overrides of validate_constraints** (Choice, Path) - One useful (Choice), one no-op (Path), showing inconsistent need

7. **Context metadata mutation in deprecation logic** - Secondary issue showing side effects of the coupling

## What Was Degraded Overall

**1. Interface Cohesion**: The ParamType interface no longer represents a single, focused abstraction. It now conflates:
   - Type conversion (core responsibility)
   - Post-conversion validation (specialized concern)
   - Diagnostic metadata collection (debugging concern)
   - Value source tracking (UI/presentation concern)

**2. Maintainability**: 
   - Every new ParamType subclass must now understand and potentially implement three additional methods
   - Implementers must decide whether to override with no-op implementations or rely on base behavior
   - Documentation burden increases significantly

**3. Code Duplication**: The pattern of overriding `get_diagnostic_info` to add type-specific metadata is repeated across 4+ classes with similar structure.

**4. Coupling**:
   - Types are now coupled to context internal structure (`ctx._type_constraints`, `ctx.meta`)
   - Deprecation warnings are coupled to type interface methods
   - Multiple concerns (validation, diagnostics, deprecation) are tangled together

**5. Testability**: Each type now has 3x more interface surface to test, much of it irrelevant to the type's core function.

**6. Cognitive Load**: Developers must understand when each method is called, why it exists, and whether they need to override it - even for simple types.

**7. API Clarity**: The separation of concerns between `convert()` and `validate_constraints()` is unclear. When should validation happen in convert vs. in the validation hook?

## Key Evaluation Signals

A proper fix should address these concrete indicators:

### Must-Have (distinguishing thorough from superficial):

1. **Interface slimming**: The ParamType base class should not contain all three new methods. At most one might be justified, but likely all three should be removed from the mandatory interface.

2. **Separation of concerns**: Validation, diagnostics, and value source tracking should be separate concerns, not baked into the type interface. Look for:
   - Extract Interface pattern (separate interfaces for optional capabilities)
   - Strategy/Plugin pattern (external validators, diagnostics collectors)
   - Delegation (types opt-in to providing extra info rather than being forced to inherit it)

3. **Reduced coupling**: The Parameter class should not assume all types support these methods. Check if the fix uses:
   - Duck typing / hasattr checks
   - Optional interfaces that types can implement
   - Separate registry/extension mechanisms

4. **No forced implementations**: Subclasses like Path should not have empty override methods just to satisfy the interface.

### Important but Secondary:

5. **Context cleanup**: The implicit contracts with `ctx._type_constraints` and `ctx.meta['_deprecated_sources']` should be removed or made explicit through better APIs.

6. **Feature independence**: Deprecation warnings should work without requiring types to implement `resolve_value_source()`.

7. **Code consolidation**: If diagnostic info is still needed, it shouldn't require 4+ nearly-identical overrides across type classes.

### Red Flags in a "Fix":

- Moving methods around without actually removing them from the base interface
- Adding abstract methods (making it worse by forcing implementation)
- Adding more conditional logic to work around the fat interface
- Introducing adapter/wrapper layers that preserve the underlying bloat

The gold standard fix would make the ParamType interface lean again, focused solely on type conversion, with optional capabilities handled through composition, delegation, or optional interfaces that types can choose to implement.

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
