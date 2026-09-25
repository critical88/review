# Injection design record — tofi per-element theme fallback logic

## Maintenance motivation

tofi is a small Wayland launcher whose look is configurable down to individual
on-screen elements: the prompt, the input line (and the placeholder shown while
it is empty), every result row (with a separate look for alternating rows), the
selected row, and the cursor can each be given their own colours, background,
padding and corner radius on top of the window-wide text and background
colours. Configuration parsing in `src/config.c` stores each of these in
per-element theme structures and records, in a set of `*_specified` flags,
which fields the user actually configured.

Whenever a field is left unconfigured it inherits from elsewhere, which is the
behaviour users expect, and the relationships are deliberately not uniform:
window-level values are the root for most elements; alternating rows inherit
from the *default result* theme so that a user configuring only result colours
still themes both row styles consistently; the cursor is stranger still,
deriving its body colour from the input theme's foreground colour and the
colour of the characters highlighted inside it from the window background.
Keeping the answer to "what does an unset value mean?" correct in one place
matters to whoever adds the next theming feature.

## Evolution being modeled

The change models a codebase after several plausible per-element theming
commits, not a deliberate act of scattering:

1. The entry module originally resolved everything once, in `entry_init`, right
   after configuration load and before the first draw. That step dates from
   when there were only window-level defaults to fill in, and the two cursor
   rules were tacked onto it as a special case.
2. As per-element options grew, changes were made where values were consumed:
   a draw function that already receives a theme can equally well derive the
   missing fields on the spot, and at the time each site was touched an
   inline "fill in the unset parts" block was the smallest locally-reasonable
   edit — the init-time resolution was not general enough to reuse, the
   renderers had no helper to call, and the immediate value was obvious in
   front of the person editing.
3. Once one theme's fallback became another theme's *configured* values
   (alternating rows), resolution order started to matter, so the per-frame
   update drivers began preparing resolved copies of the themes before
   drawing, and themed draw helpers gained fallback parameters.
4. A colour-propagation concern at config-load time — "when the user sets the
   general text colour, every element that would inherit it should be brought
   up to date" — was patched where the value originates, in the parser, which
   duplicates the same relationship on the write side.
5. Once every consumer resolved locally, nothing depended on the init-time
   pass any more, and the ordinary cleanup instinct removed the helper and the
   `entry_init` block, completing the drift: the decision now lives at its
   consumers and at one producer.

## Overall design

The single semantic decision is "what value does an unset theme field take".
The change removes that decision from the module that owns the theme state and
re-instantiates it at every site that needs the answer, in two shapes:

* **Read-side resolution blocks** in both rendering backends: each fragment
  copies the incoming theme and fills its unset fields from some fallback,
  guarded by the same `*_specified` flags the parser maintains. The
  chain-relevant relationships are expressed locally: the frame drivers
  pre-resolve the default-result theme (because alternating rows inherit from
  it) and the selection theme (because the selected-row two-pass draw consumes
  it directly), while the input-line helper re-derives the cursor rules.
* **One write-side propagation block** in the configuration parser, which
  eagerly keeps six per-element themes' foreground colours in sync when the
  window-wide text colour is set.

Because tofi keeps two independent text renderers with the same draw
responsibilities, anything done to one renderer's draw paths has its natural
counterpart in the other, so each fragment exists twice. Values are preserved
exactly: every fragment reproduces the relationship the init-time pass used to
apply — window-level root defaults (window text colour, fully transparent
background, zero padding, zero corner radius) for prompt, input, placeholder,
normal results and selection; the default-result theme as the second tier for
alternating rows; input-derived colours for the cursor.

## Per-location rationale

### `src/entry.c` — `entry_init` and the removed helper

*What changed*: deletion of the static `apply_text_theme_fallback` helper and
of the `entry_init` block that constructed the window-level default theme,
applied it to six themes, and applied the two cursor fallback rules.

*Why this location and form*: `entry.c` owns `struct entry` and all theme
state, the natural home of the decision; the removed block was the only code
still performing the resolution up front, and once the consumers had their own
copies it read as dead weight, so removing it whole is the shape that cleanup
commits take. `entry_init` keeps its real responsibilities: window geometry,
cairo surface and clip setup, backend dispatch and the initial frame.

*Production role*: the entry module now hands the themes to the renderers
exactly as configured, with no derived values, so nothing outside the
renderers (and the parser fragment) knows what unset means.

### `src/config.c` — `parse_option`, window-wide text-colour handler

*What changed*: after storing the parsed colour on the entry, the handler also
writes it into the foreground colour of the prompt, input, placeholder,
default-result, alternate-result and selection themes whenever their
foreground was not explicitly configured, guarded by the existing
`*_specified` flags.

*Why this location and form*: the parser is where the inherited-from value
originates, so a maintainer chasing reports like "text-colour doesn't reach
elements configured later in the file" reaches for the smallest correction at
the value's origin; reusing the existing flags keeps each three-line block
looking like ordinary parser bookkeeping.

*Production role*: a write-side mirror of the read-side fragments — changing
what unset text colours inherit now also requires touching option parsing.

### `src/entry_backend/harfbuzz.c` — three draw-side sites

* `render_text_themed` (draws every themed text: the prompt and each result
  row) gained a fallback theme parameter; it copies the incoming theme and
  fills the four unset field groups from that fallback, then draws everything
  from the local copy.
* `render_input` (draws the input line in all three buffer shapes, plus its
  placeholder and cursor) changed its first parameter from the backend-specific
  harfbuzz state to the whole entry, because the window-level colours it needs
  live on the entry; it builds the window-level default theme locally, resolves
  the incoming theme against it, and applies the cursor rules (body colour from
  the input theme's foreground when configured, else the window foreground;
  highlighted-character colour from the window background).
* `entry_backend_harfbuzz_update` (the per-frame driver) builds the local
  window-level default theme plus pre-resolved copies of the default-result
  theme and the selection theme, passes an appropriate fallback to each themed
  draw (the resolved default-result copy only for alternating rows), and feeds
  the resolved selection copy to the two-pass selected-row highlight loop it
  already contained.

*Why these locations and forms*: these are the three grain sizes at which a
renderer consumes theme state — a leaf text-drawing helper, an input-plus-
cursor helper, and the driver that also selects which theme each row uses.
Each fragment is self-contained, commented as if it were the whole rule, and
individually reads like a reasonable local decision; the effect only appears
when the same question is asked on another path.

*Production role*: this backend now decides effective values at draw time for
its entire drawing surface (used when the configured font name is a readable
font file path).

### `src/entry_backend/pango.c` — the mirrored three sites

* `render_text_themed` moves its resolution to the top of the body and rebinds
  the local `theme` pointer to the resolved copy, so the long pre-existing body
  underneath keeps reading `theme->...` unchanged.
* `render_input` gains the entry as its first parameter and mirrors the
  input-theme and cursor resolution.
* `entry_backend_pango_update` mirrors the driver-side preparation of the
  default-result and selection copies, the per-row fallback selection, and the
  two-pass selected-row draw fed from the resolved selection copy (padding and
  background box drawing included).

*Why mirror*: the Pango and HarfBuzz backends implement identical draw
responsibilities independently and are selected by the same font setting, so
any change to one renderer's draw paths has its natural counterpart in the
other — the mirrors exist precisely because the two are supposed to stay in
tandem.

*Production role*: identical to `harfbuzz.c`, for the font-name (non-file)
configuration path.

## Boundary of the change

Deliberately not touched, all left exactly as before: the cursor *thickness*
fallback in both backends (a `*_specified`-style flag that computes a
font-metrics default — theming-shaped, but an unrelated rule), window, border
and outline drawing in `tofi.c` (window-level colours only), padding/clip
geometry handling, and the `selection_highlight_color` visibility condition
that decides whether a row renders through the selection theme at all.

Two incidental edits landed on lines the change was already rewriting: the
doubled semicolons on the row-theme selection statements in both update drivers
collapse to single ones, and the `render_input` parameter change in the
HarfBuzz backend is the signature-level counterpart of the state its body now
needs.
