# Injection design record — react-date-picker custom-input date-part handling

## Maintenance motivation

`react-date-picker` renders the custom (non-native) date input as three small
part views — a year field, a month field (either a number field or a select,
depending on the locale's month names), and a day field — composed by
`DateInput.tsx`. When the user types into any of them, the component must
answer one question: "given the raw text (or option value) each part view
currently holds, which Date did the user mean?" On top of the base commit's
behavior, requests kept arriving that all touch that translation:

- a report that an input restricted with short formats such as `dd.MM`
  (no day `dd`, or no year `MM.yyyy`) should still be submittable in the same
  gesture as full formats;
- a wish from integrators to render a subset of the parts (month only, month +
  year), where the parts that are not rendered obviously hold no value;
- product feedback that the day field should stop clamping everything to a
  plateau of 31 and respect the currently chosen month (end of January versus
  end of February), including leap Februaries;
- translation text corrections for locales using zero-padded or short years.

What all of these requests have in common is a small "date-part codec": a rule
for turning the raw values of the part controls into numbers, a decision about
which value to use when a part holds nothing, and a rule for presenting the
currently selected date back into each control's text.

## Modeled development evolution

The change models how this grew incrementally across a component library
release cycle, without anyone planning a shared abstraction:

1. A first iteration made each part view self-sufficient: every component that
   renders a part keeps the codec knowledge for that part next to its JSX, as
   a pair of small named helpers ("what number do I use for this part?" /
   "what text do I show for this part?"). Locally this reads well — the day
   logic lives with the day component.
2. When the select variant of the month view was revisited, its placeholder
   option produced a slightly different raw value shape, so it received its own
   guarded decoding rather than a shared abstraction.
3. When the day field's max bound had to respect the chosen month, the day
   view needed the month's decoding rule; the nearest implementation was the
   month view module itself, so the day view imports from its sibling.
4. When `DateInput.tsx` was later generalized, the coordination grew into
   module-scope tables that map each part name to the helpers its view
   exports. The handler no longer names any part explicitly; it walks the form
   elements, and resolves the strategy for each element at runtime.

Each step is a plausible thing to merge on its own. The cumulative result is
that the codec is spread across the per-part views and glued together from the
coordinator, so the same policy decision is restated in several modules and
any evolution of it has to be repeated there.

## Overall design of the change

- **Scope**: the per-part view components under
  `packages/react-date-picker/src/DateInput/` and their coordinator
  `packages/react-date-picker/src/DateInput.tsx`. No test files, no shared
  modules under `src/shared/`, no other packages are touched.
- **Behavior**: the external contract of the picker is unchanged — the value
  submitted through `onChange`, the moments of submission, the day clamping
  by month length, disabled-date handling, and rendering output behave
  exactly as on the base commit.
- **Fragments**: each part view defines and exports two small named helpers
  at module scope — a decoder from the raw control value to a number
  (selecting the fallback for an empty part: the first day of the month, human
  month 1 = January, or the current year), and an encoder from the value to
  the control's display text. The select variant of the month exports only a
  decoder, shaped for its non-numeric placeholder value.
- **Coordination**: `DateInput.tsx` registers the decoders and encoders in
  module-scope maps keyed by part name, and — inside its change handler —
  resolves, per form element and at runtime, whether a given `name`
  takes its decoding from the input-shaped table or the select-shaped table,
  then applies the low-to-high per-part strategies to build the candidate
  date. The sync effect reads the encoder map to push each part's display
  text back into the controlled fields.
- **Cross-module data**: the day view's max-day computation imports the month
  decoder from the month view module, so the month policy is also consumed
  from a different part view, not only from the coordinator.

## Per-location walkthrough

### `src/DateInput/DayInput.tsx`

**What changed.** Gains two module-scope named exports —
`dayNumberToUse(rawDayValue)` returning `Number(rawDayValue || 1)`, and
`dayStringFromValue(nextValue)` returning `getDate(nextValue).toString()` —
plus one behavioral rewire: the month-day maximum is now computed as
`getDaysInMonth(new Date(Number(year), monthNumberToUse(month) - 1, 1))`,
where `monthNumberToUse` comes from `./MonthInput.js`.

**Why this site and shape.** The day view is where the chosen-month-dependent
bound already lived (`currentMonthMaxDays`), so the month-aware rewire is a
natural extension of an existing computation: the bound previously assumed a
31-day plateau or a crude numeric read; now it derives the month index through
the month view's own decoding rule (human month minus 1) instead of a local
duplicate. `dayNumberToUse` is the smallest plausible owner of the day
fallback: 1 (the first of the month) is a day-specific default, so a reader
would expect it defined beside the day rendering rather than centrally.
`dayStringFromValue` mirrors that locality for presentation.

**Production role.** Recomputes the legal day range for the currently visible
month (next to `maxDate`/`minDate` clamping via `safeMin`/`safeMax` in
`shared/utils.ts`, unchanged here), and owns the day half of the part codec:
what an empty day means when composing a date, and what text the day field
presents for the current value.

### `src/DateInput/MonthInput.tsx`

**What changed.** Gains two named exports — `monthNumberToUse(rawMonthValue)`
returning `Number(rawMonthValue || 1)`, and `monthStringFromValue(nextValue)`
returning `getMonthHuman(nextValue).toString()` — inserted above the
component definition; nothing else in the module changes.

**Why this site and shape.** The month view is the natural home for the
January fallback from a maintainer's chair ("months count from 1, if nothing
is chosen it is January"), and `getMonthHuman` is the human-ordering getter
the month rendering already leans on. The encoder gives the month field its
presentation counterpart next to the decoder, so both halves of the codec for
this part sit side by side and the module reads self-contained.

**Production role.** Decides the month of the candidate date for
numeric month fields, including when the month field is not rendered (the
"month first" format case), and produces the month text the field shows for
the current value. Because `DayInput` also imports `monthNumberToUse`, this
module additionally serves as the month-policy provider for the day view's
bound computation.

### `src/DateInput/MonthSelect.tsx`

**What changed.** Gains one named export, `selectMonthNumberToUse(rawMonthValue)`,
with a guarded shape: `const monthNumber = Number(rawMonthValue); return
Number.isNaN(monthNumber) ? 1 : monthNumber;` — instead of the logical default
used by its sibling input.

**Why this site and shape.** For locales with long month names the month part
renders as a `<select>` whose placeholder option carries a non-numeric value,
so a decode must tolerate an unparseable selection rather than an empty one.
A maintainer touching only the select variant reaches for a `Number.isNaN`
guard, which is why this fragment looks different from the others it
parallels: same policy (January when nothing is usable), different input
reality. Keeping it local to the select module (rather than reusing the month
input's decoder) is the locally-plausible choice — the two views report
different raw value kinds, so duplicating the tiny guard next to its only
consumer reads as harmless specialization.

**Production role.** Provides the month decode for select-shaped controls,
which the coordinator consumes only when the month part happens to be a
select, and which is therefore invisible in a purely static per-module
reading of the month handling.

### `src/DateInput/YearInput.tsx`

**What changed.** Gains two named exports — `yearNumberToUse(rawYearValue)`
returning `Number(rawYearValue || new Date().getFullYear())`, and
`yearStringFromValue(nextValue)` returning `getYear(nextValue).toString()`.

**Why this site and shape.** The year fallback is the only one that reads the
clock rather than a fixed constant: without a year, "the date the user meant"
is understood to be in the current year, which is what makes short formats
(`dd.MM`) coherent. Placing the
dynamic default beside the year rendering keeps the year-specific
interpretation (`new Date().getFullYear()`) with the year field instead of a
shared clock-reading helper. The encoder is symmetrical with the input and
day views so the field's display matches its decode, as a year analogue of the other two part encoders.

**Production role.** Decides the year of the candidate date (including the
current-year completion for formats that never render a year), and produces
the year text the field holds for the current value, including how the
value appears before the user edits or clears it.

### `src/DateInput.tsx` (coordinator)

**What changed.** Three groups of edits:

1. **Imports.** Each per-part import now also pulls that part's helpers
  (`import DayInput, { dayNumberToUse, dayStringFromValue } from
  './DateInput/DayInput.js'`, and similarly for the month input, month select
   — decoder only — and year input). The `@wojtekmaj/date-utils` import drops
   `getDate`, `getMonthHuman`, and `getYear`: the coordinator no longer
   reads the value per part itself.
2. **Module-scope codec tables.** Three explicit types (`PartName`,
   `PartNumberStrategy`, `PartTextStrategy`) plus three registrations:
   `inputPartNumberStrategies` (day/month/year decoders from the input
   modules), `selectPartNumberStrategies` (currently only the month select's
   decoder — a partial map, since only the month has a select variant), and
   `partTextStrategies` (the three encoders). The maps tie a part name to the
   module that owns that part's codec without the component naming the parts
   in its body.
3. **Runtime dispatch in the change handler and encoder use in the effect.**
   The getter-side effect replaces the three direct `getYear(value)` /
   `getMonthHuman(value)` / `getDate(value)` reads with a loop over
   `partTextStrategies`, calling each encoder and pushing the text into the
   per-part state. The change handler builds `values` by walking the rendered
   form elements and reading each element's value at runtime; it then starts
   from a copy of `inputPartNumberStrategies` and, per element, when the
   element does not expose `valueAsNumber` — the runtime shape criterion that
   separates a `<select>` from an `<input type="number">` in this DOM —
   substitutes the select-shaped decoder registered for that part where one
   exists. Finally it resolves the three numbers through the strategy
   functions and constructs the candidate date with
   `setFullYear(year, monthIndex, day)` plus a midnight normalization.

**Why this site and shape.** The coordinator is the only place that sees all
part views at once, so it is where the fragments must be stitched — but the
stitch is indirect on purpose: a maintainer generalizing "handle the parts
uniformly" writes part-keyed tables, and distinction between input-shaped and
select-shaped decode is a per-element runtime question (`'valueAsNumber' in
formElement`), not a per-import static one. Following the code therefore
requires walking the event-time execution: which elements are rendered,
which of them are selects, and what the tables resolve to for their `name`s.

**Production role.** Remains the single owner of *when* composition happens
(value sync in both directions, native-input fallback, onChange emission,
value clamping with `safeMin`/`safeMax` against the min/max dates), while the
per-part views now dominate *how* a raw part value becomes a number and how
the value becomes part display text. The tables also make the boundaries
soft: adding a part would require touching a view module, all three tables,
and the dispatch loop.

### Files deliberately left as they are

`shared/utils.ts`, `shared/dates.ts`, and `shared/dateFormatter.ts` retain
their roles (clamping, interval math, Intl formatting) — they are adjacent
helpers, and plausible places a maintainer might glance at, but they do not
carry per-part codec knowledge; `DatePicker.tsx`, `NativeInput.tsx`,
`Divider.tsx`, and `index.ts` keep their composition-only responsibilities.
The per-part spec files are untouched, so the case keeps the original
behavioral safety net in place.

## Diff statistics

5 production files changed; 113 insertions, 14 deletions across 10 hunks.
Four of the files are the per-part view components (two of them gaining two
named helpers each, one gaining a select-shaped helper pair member, one
gaining a select-shaped decoder only); the fifth and largest hunk set is the
coordinator's import routes, codec tables, dispatch, and effect loop.
