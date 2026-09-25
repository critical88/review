# Injection design record — slug pipeline staged behind one uniform contract

## Maintenance motivation

The package grew a fixed sequence of text operations for turning arbitrary
input into a slug: user supplied substitution maps, a per language
substitution table, ASCII transliteration, lowercasing, two kinds of length
limiting, unauthorized symbol replacement, dash collapsing, edge trimming and
an optional timestamp suffix. For years that sequence lived inline in a single
function that read the package options directly. Two recurring requests put
pressure on that shape: downstream users wanted the phases to become a
documented, uniform hook surface they could reason about and eventually extend,
and maintainers wanted the per language dispatch (a large switch inside the
generation function) to live with the language tables it selects from instead
of inflating the main entry point. Both requests are ordinary library
maintenance, and neither of them asks for a single giant abstraction.

## Evolution being modeled

The diff models the most common way such an abstraction actually appears: not
as one deliberate architecture, but as the sum of several plausible smaller
steps that together lock the design in. A first stage is written out by hand
against a full "do everything" contract while the shape is still being
explored. Once a second and third stage are added, the pass-through clutter is
extracted into a shared default type that every later stage embeds, so the
clutter stops being visible while remaining obligatory for every participant.
The public entry function then becomes a thin loop that advances the
registered stages through each phase, and the language switch finally moves
onto the contract as just another capability. Each step reads as local
progress; the combined result couples every participant of the pipeline to
every capability of the pipeline.

That evolution is modeled rather than explained in the code: the shipped
state shows the hand-written stage, the extracted shared defaults, the
registry driven entry point and the relocated language dispatch side by side,
the way a repository looks after the fact.

## Overall design

The generation sequence is refactored into a staged pipeline:

- `pipeline.go` (new) declares the interface `slugStage`, the shared default
  type `noopStage`, the stage registry, the phase advancing function, and the
  stages with no stronger natural home (custom substitution, ASCII
  normalization).
- `slug.go` keeps the public API (`Make`, `MakeLang`, `Substitute`,
  `SubstituteRune`, `IsSlug`, all option variables) and gains the stages that
  encode the options declared in this file: case folding, the two length
  limiters, separator cleanup and the timestamp suffix.
- `languages_substitution.go` keeps the language tables and gains the stage
  that owns the language dispatch, so the big per language switch finally
  sits next to the data it selects.

The interface itself names twelve capabilities: a diagnostics name, all ten
text phases of the generation sequence, and shape validation. The registry
lists seven concrete stages, each of which owns one to three phases; the
phase loops in `applyStages` advance every registered stage through every
phase, so each loop walks full contract values while invoking exactly one
capability per loop.

## Per location rationale

### `pipeline.go` — the uniform contract

The contract is placed in its own file because it is meant to be the public
face of the pipeline subsystem: everything else in the pipeline refers to it.
Twelve capabilities were included because the temptation it models is real:
while unifying the phases, each existing operation in the sequence is an
operation the pipeline "can perform", so each one becomes a capability of the
one contract rather than being questioned. The interface also gained two
capabilities with no pipeline role at all: `StageName`, carried for a
diagnostics surface that was sketched for the registry, and `IsValidSlug`,
carried so that "any stage can validate" — both are required of every stage
even though the phase machinery never uses them. Declaring the contract
narrowed to a package internal (`slugStage`, not an exported type) matches the
intermediate state this project would plausibly ship before exposing anything.

### `pipeline.go` — the shared default type `noopStage`

Once the first hand-written stage existed, the identical pass-through bodies
for the phases a stage does not own would obviously be extracted, and
`noopStage` is that extraction: one default implementation of every capability
that returns its input unchanged, plus the one real validation implementation
that the exported `IsSlug` reaches through a concrete value. Modeling the
defaults as an embeddable type rather than generated code or closures is the
idiomatic Go shape for "inherit the parts you do not care about", and it is
what makes the obligations survive future stage additions without anyone
rereading them. Keeping the real validation body here — instead of a plain
function — reflects the stated intent that validation is a capability every
stage carries.

### `pipeline.go` — `customSubstitutionStage` (hand-written implementor)

The oldest stage applies `CustomRuneSub` before `CustomSub`, exactly as the
inline code did, and is written out against the entire contract method by
method, including explicit pass-through bodies for the nine phases it does not
own and a delegation for validation. This placement and shape are deliberate
fossil record: this is what the first participant looked like before the
defaults were extracted, and custom substitution is phase number two of the
real sequence, so it would plausibly be the first thing moved behind a uniform
contract. Its production role is identical to the code it replaced — apply the
user's maps, runes first — and its stub bodies document, in the most explicit
form this design takes, what the contract costs a participant that needs one
phase out of twelve.

### `pipeline.go` — `asciiNormalizationStage`

The transliteration step is the phase with no interesting configuration, so
it embeds the shared defaults and overrides a single capability that calls
`unidecode.Unidecode`. It demonstrates the mature pattern the design settled
into: the noise of the first stage is gone from view, and a new participant is
a three line type declaration. Its production role is the ASCII normalization
phase of the sequence, unchanged.

### `pipeline.go` — registry and phase advancing (`pipelineStages`, `applyStages`)

The registry is a package level slice of the contract, listing the seven
stages in pipeline order. `applyStages` advances the registered stages
through the sequence with one loop per phase, in the exact phase order the old
inline code used, and `MakeLang` calls it after trimming the input. The
phase-major shape — every stage through phase one, then every stage through
phase two — is the form that lets any stage "hook into any phase", which is
precisely the property the contract's doc comment advertises; it is also the
form that makes every loop accept a twelve capability value to call one
method. This placement keeps `MakeLang` thin and keeps the pipeline order in
one readable function.

### `slug.go` — `caseFoldingStage`, `lengthGuardStage`, `sanitizingStage`, `uniquifyingStage`

These four stages encode the package options declared in this file:
`caseFoldingStage` applies the `Lowercase` option; `lengthGuardStage` owns
both length phases, cutting at `MaxLength` when smart truncation is off and
cutting on a word boundary when it is on; `sanitizingStage` owns
unauthorized symbol replacement, dash collapsing under
`DisableMultipleDashTrim` and edge trimming under `DisableEndsTrim`;
`uniquifyingStage` appends the timestamp suffix under `AppendTimestamp`.
Putting them beside the variables they read keeps option and behaviour
together, and their bodies are the original conditionals with the phase
framing removed. Each of them embeds the shared defaults and carries the
other nine phases and both diagnostics capabilities without any behaviour.

`lengthGuardStage` and `sanitizingStage` are the only participants that own
more than one capability, because the length limits and the separator cleanup
genuinely come in pairs in this package; their remaining capabilities are
carried for the contract's sake, not for their own.

### `slug.go` — `MakeLang` and `IsSlug`

`MakeLang` keeps its signature, its doc comment and the initial trim, then
advances the pipeline and returns. `IsSlug` keeps its exported contract but
now reaches the shared validation capability through the concrete default
value instead of inlining the check, which matches the design claim that
validation became a capability delivered by the pipeline's shared carrier.
Both functions keep their production role; the only change is who performs the
work.

### `languages_substitution.go` — `languageSubstitutionStage`

The language dispatch moves from the middle of the generation function onto
the contract as the `SubstituteLanguage` capability, implemented as the same
switch over the same ISO codes with the same `'en'` fallback, returning the
substituted string instead of reassigning a shared variable. The stage lives
in this file because the twenty-one tables it selects from live here, which
was half of the original motivation. It embeds the shared defaults like the
other relocated stages, so the language phase gains a contract participant
while the tables themselves stay untouched data.

## Structural variation, deliberately present

The injection deliberately uses three different implementation shapes for the
same relation, because the evolution that produces such a design produces
exactly this mixture: one stage written out by hand against the whole
contract, six stages that embed a shared pass-through default, and a registry
consumer that walks full contract values one capability at a time. The
diagnostics capability and the validation capability are carried by every
participant through different mechanisms — literal overrides on some stages,
the shared default on the rest — and the validation implementation is
genuinely executed, but only through concrete typed access, so usage
inspection and contract obligations no longer agree. No artificial or
never-executed fillers were added beyond the two unused capabilities whose
planned consumers (the diagnostics surface, stage based validation) are named
in the code's own documentation.

## What was deliberately not done

The exported functions `Substitute` and `SubstituteRune` were not pulled into
the contract: they are public API called directly by downstream users and by
the substitution phases. The language tables, `doc.go`, the unexported
`smartTruncate` and `timestamp` helpers and the test file were left as they
were; the helpers stay plain functions because the package's own benchmarks
call `smartTruncate` directly, and reshaping them would change the package's
internal surface without serving the pipeline story.
