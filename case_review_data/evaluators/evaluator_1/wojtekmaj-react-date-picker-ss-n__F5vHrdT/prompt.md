# Consolidate per-part date handling in the custom date input

## Where this comes from

react-date-picker's custom (non-native) date input renders the selected date as
three small controls — a year field, a month control, and a day field — and
keeps them synchronized with the picker's value. Over a series of feature
requests this grew support for partial formats (for example `dd.MM`, where no
year control is rendered at all), for locales that render the month as a
select instead of a number field, and for a day field whose maximum respects
the currently chosen month.

It is now hard to evolve. The rules for translating between the value and
these controls live next to each control component, so a change to the
policy means repeating small edits across several components instead of one
adjustment in a single place. This covers two directions of the translation:
how the raw content of a control becomes a part of the constructed date —
including what to use when that part is absent (a format without a year, a
month select still on its placeholder, an empty part field) — and how the
current value is presented back as each control's content.

## What we want

Give this whole "part control content ⇄ date part" policy one shared home in
this package's source so that:

- decoding a part control's raw value into a number, choosing the default for
  a part that is absent or unresolved, rests in a single shared
  implementation instead of copies inside the part control components;
- presenting the value back into each control's text resolves through the
  same shared implementation, not per-control re-derivations;
- a part that can be rendered in more than one shape (the month, as a field
  or as a select) is handled by one shared policy rather than per-shape
  copies;
- the per-control components become thin rendering concerns again, and any
  future policy tweak (a new default, a new part rule) is a one-place change.

## What must stay exactly as it is

- The public API of the package is unchanged: component and hook exports,
  prop names, prop types, and the exported types of the entrypoint stay as
  they are.
- `onChange` keeps firing at the same moments with the same `Date` values for
  the same interactions, in every rendering configuration. In particular:
  - with a format that renders no year control (`dd.MM`), submitting the
    month/day uses the **current year**, the typed human month, and the typed
    day;
  - a month select left on its placeholder contributes **January**, same as
    an empty month field;
  - a missing or empty day contributes **the 1st of the month**;
  - day range clamping still respects the entered month length (including
    leap Februaries) as well as the `minDate`/`maxDate` bounds;
  - the native (browser) date input path behaves exactly as before;
  - constructed candidate dates keep their midnight time-of-day
    normalization.
- The date-utility helpers on which behavior depends (`@wojtekmaj/date-utils`,
  the formatting helper used for aria labels, and the existing shared
  clamping helpers) keep their roles and signatures.
- The package still type-checks and builds, and the complete unit suite keeps
  passing with no modifications to the spec files: closing this maintenance
  pain is expected to leave both the observable behavior and the tests
  untouched.

While consolidating, keep an eye on runtime flow rather than static
appearance: which controls are rendered under a given format, and how each
rendered control's content is read back out, decide which policy fragments
actually participate, and the consolidation is only complete when all of
them resolve through the shared home.
