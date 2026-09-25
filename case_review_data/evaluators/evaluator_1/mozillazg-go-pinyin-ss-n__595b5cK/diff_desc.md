# Injection design record — go-pinyin style rendering scatter

Repository: `github.com/mozillazg/go-pinyin` at `0562d652b86dd0540f53fdfc8475b18aa2d42c9d`
Language: Go. Changed surface: `pinyin.go`, `phonetic_symbol.go`, plus new files `pinyin_tone.go` and `pinyin_finals.go` (all in the root module).

## Maintenance motivation

go-pinyin sells one thing: render a Hanzi syllable according to one of ten
exported "styles" (`Normal`, `Tone`, `Tone2`, `Tone3`, `Initials`,
`FirstLetter`, `Finals`, `FinalsTone`, `FinalsTone2`, `FinalsTone3`; four
additional decades-old uppercase aliases point at the same values). The style
vocabulary keeps growing — `Tone3`/`FinalsTone3` were appended years after the
first eight styles, and every release of the underlying pinyin-data has forced
small corrections to how a family renders (y/w initial handling, j/q/x finals
spelling, nasal characters with no initial to strip, digit placement).

The costly recurring question is "how many places must agree when a style's
rendering changes, or when a new style joins the set?" A style is not a
syllable-local detail: it constrains the dictionary candidates a character
yields, the tone-mark decoding, where tone digits sit, whether the head
letter or the final is kept, and which duplicates count as repeats after
rendering. The library currently answers that question with a single
syllable-fixup function (`toFixed` in `pinyin.go`), which is the only place
that reads `Args.Style` and decides rendering. When someone wants the answer
to be "one place", that ownership is the thing to protect.

The modeled situation is a library that accreted style-aware behavior at the
point of need instead of in the rendering owner: each maintainer working on a
different stage of the conversion pipeline added the style knowledge they
needed right where they were working because that location had the data at
hand. The result reads as reasonable local engineering — every fragment sits
next to the data it uses — but the ten styles' behavior is now jointly
defined by several functions in several files, and there is no single place
that describes how a style is rendered.

## Overall injection design

The clean rendering decision chain is one function:

per candidate syllable → (Initials shortcut) → replace tone-marked characters
per style family → reposition tone digits for the digit-at-end styles →
head-letter truncation or finals extraction (with the nasal special case) →
order-preserving de-duplication.

The injection distributes that one policy across five owner locations. Each
fragment is small (2–15 lines), keeps its original comment texture (Chinese
comments matching the file's existing style), and looks locally motivated by
the stage it lives in. The fragments partition the pipeline by purpose
rather than duplicating one another, and the per-syllable transformation
order (symbol decoding, then digit repositioning, then finals extraction,
then collection-time truncation and de-duplication on transformed values) is
retained, so every candidate syllable passes through the same effective
sequence of transformations as before. The per-syllable step order and the
family groupings are the exact ones the library shipped with.

Two structural seams are deliberate:

- **per-stage ownership**: the five fragments are hosted by five different
  pipeline roles (character-level lookup, symbol decoding, digit placement,
  collection, finals extraction), so understanding the resulting flow means
  following the syllable, not reading one function;
- **feature-file gravity**: two fragments sit in small feature-specific files
  that a reasonable maintainer could have created for a "tone numbering" and
  a "finals" concern, and the data those fragments need (regexes, exception
  tables, helper functions) moved with them, which is what makes each
  fragment look self-contained.

## Cluster-by-cluster rationale

### `pinyin.go` — pipeline stages absorb their own style decisions

This file owns the whole conversion pipeline (`SinglePinyin` → `applyStyle`
→ `toFixed` → `Pinyin`/`LazyPinyin`/`Slug`).

- **`SinglePinyin` (Initials contraction)**: the per-character entry point
  expands `PinyinDict` values (or a custom `Fallback`) into a candidate list.
  It now contracts every candidate to its initial (`if a.Style == Initials`
  around an `initial(v)` loop) while it still holds the raw list, which is
  the first stage where the style could plausibly matter and a natural spot
  for a maintainer to short-circuit the obvious style. *Why here and this
  shape*: the candidate list is materialized exactly here, so "just strip
  initials immediately" reads as an efficiency-scented local decision, and
  the loop form (in-place element replacement over `pys`) fits the
  surrounding code (the existing `initial()` helper and the subsequent
  `pys[0]` heteronym selection both iterate/manipulate candidate slices).
  *Production role*: dictionary/fallback expansion plus heteronym selection —
  the place a new entry mode would touch.
- **`applyStyle` (FirstLetter truncation)**: the collection loop that renders
  each candidate and de-duplicates order-preserving results now takes the
  first rune (`if a.Style == FirstLetter` cutting `v` to one character)
  itself. *Why here*: the loop already ends with "build the final list", and
  taking the head letter of an already-rendered syllable looks like a
  presentation concern of the collector rather than a rendering rule.
  *Production role*: result collection, heteronym de-duplication and the
  single place all downstream consumers feed from.
- **`toFixed` (converted into a sequencer)**: the previous single owner of
  the whole policy keeps no conditional style decision of its own and only
  threads a candidate through the three fragment calls. *Why*: this is the
  clearest way the scatter forms in real code — the original owner hollows
  out and becomes a mediator, so no remaining function holds the whole
  story. *Production role*: per-syllable fix-up coordinator; keeps the
  original syllable (`origP`) alive for the nasal special case, which is
  exactly the kind of data pin that makes one suspect there was once a single
  owner here.

The meta constants, default configuration variables, exported API functions
(`Pinyin`, `LazyPinyin`, `Slug`, `Convert`, `LazyConvert`, `NewArgs`), and the
style constant vocabulary in this file are untouched: the surface that
changes is the rendering decision, not the library's contract.

### `phonetic_symbol.go` — symbol decoding adopts the style family policy

The tone-mark map (`phoneticSymbol`) already lived here; the tone-symbol
decoding closure that used to be nested in the old single owner moves in as
`decodePhoneticSymbol(p string, a Args) string`, together with the
`rePhoneticSymbolSource`/`rePhoneticSymbol` construction and the `reTone2`
digit-stripping regex it shares a family policy with. *Why here and this
shape*: of the whole policy, "which tone form does a style get" is the part
whose inputs (the symbol table) literally live in this file, so "keep symbol
decoding next to the symbol table" is an easy argument to accept in review.
The fragment preserves the family grouping verbatim: `Normal`/`FirstLetter`
/`Finals` strip the digit from the mapped symbol, the four numeric styles
keep digits (that grouping must stay in agreement with the digit-placement
fragment), and the default keeps tone marks on the head. *Production role*:
the raw-dictionary form (`zhōng`) into per-style written form (`zhong`,
`zho1ng`) — the bridge between data and rendering.

### `pinyin_tone.go` (new) — the numeric-tone feature owns digit placement

A maintainer adding code for the digit-at-end styles needs one regex and one
case pair; the natural evolution is a small feature file rather than
enlarging the core. `repositionToneNumber(p string, a Args) string` carries
the `reTone3` regex (previously at the top of `pinyin.go`) and the
`Tone3`/`FinalsTone3` membership test that moves a mid-syllable tone digit
to the end. *Why here and this shape*: numeric tone placement is one of the
library's newest style features (the `Tone3` pair of styles), so housing it
in its own file mirrors how such features grow at the edge of a small
library; the fragment genuinely depends only on its local regex, which makes
it look self-contained and hides that its case list must agree with the
decoding fragment's keep-digit case list two files away. *Production role*:
digit-form normalization between decoding and final-shape extraction.

### `pinyin_finals.go` (new) — the finals feature takes over finals semantics

The finals rendering is the largest fragment: `extractFinals(origP, p string,
a Args) string` holds the `Finals`/`FinalsTone`/`FinalsTone2`/`FinalsTone3`
membership switch plus the nasal special case (`ḿ ń ň ǹ` keep their nasal
character because there is no initial to strip), and the implementation moves
in with it: `final()`, `handleYW()`, `finalExceptionsMap`,
`reFinalExceptions`, and `reFinal2Exceptions` relocate from `pinyin.go`.
*Why here and this shape*: of the ten styles, the four finals styles have the
richest special-case data (the j/q/x → v spellings, y/w contraction), and
that data moving together with its dispatch is the most plausible "the finals
module" story of the whole change; keeping `final()`/`handleYW()` reachable
from here (rather than near the `initial()` helper in the core) is what
makes the module look complete and self-justified. *Production role*: final
extraction and the two legacy exception systems that downstream users
complain about when a finals rendering bug appears.

### Deliberate structural variation across the clusters

The fragments intentionally differ in mechanism and in how knowledge is
split: one is an in-place list rewrite in an entry function; one is a
closure-with-family-table next to its data; one is a feature helper with a
local regex; one is a collection-stage guard; one is a feature module with
imported special-case data. Four of the five reference a *different* subset
of the same ten-style vocabulary, and two of the style families are now
jointly defined by fragments in different files (which styles receive digit
form is decided next to the symbol table, while where those digits move is
decided in the tone file; first-letter output needs both the decoding
fragment's strip-digits grouping and the collector's truncation). The
variation is not cosmetic: it makes a single scan of the constants or a
single call-graph hop insufficient for reasoning about where style behavior
lives, and it mirrors how scattered ownership actually forms — stage by
stage, not by template.

## Natural false trails (materially unchanged)

Several neighbors look style-related but carry no conditional style decision
and remain as they were: the style constant blocks (including the deprecated
uppercase aliases) in `pinyin.go`; `initialArray`/`initial()`; the CLI's
`styleValues` flag table in `cli/pinyin/main.go` (a separate module pinned to
the published release, unbuilt by the root commands); the generated
`PinyinDict`; and the arguments/defaults handling in `NewArgs`, `Convert`,
`LazyConvert`. These bystanders are the reason a conceptual read of "whoever
mentions a style" over-collects.

## Scope boundaries

Explored but not included: the CLI module (outside the root-module build/test
contract), generated dictionary data, the `_tools` generator, and the
`Args`-defaulting concern (a different responsibility that predates this
change). The five chosen owners cover every stage that the clean rendering
chain exercises per candidate syllable — lookup, decoding, digit placement,
finals extraction, and collection — and each inherits its own subset of the
style vocabulary, which is the saturation point for this responsibility in
this module.
