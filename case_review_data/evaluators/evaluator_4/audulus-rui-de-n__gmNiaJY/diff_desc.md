# Injection design record — rui

## Maintenance motivation

rui's README describes the library as an "Experimental Rust UI library ... Early
days", where layout, dirty tracking, and event dispatch are exactly the
subsystems still being tuned. Anyone who has worked on a layout engine knows
the practical pain that motivated this change: you can *read* the layout code
all day, but when a stack sizes wrong or a region fails to mark dirty, what
you want is to *see* it. SwiftUI and Flutter both ship debugging overlays for
precisely this reason, and rui had nothing comparable — the only observability
hooks were a handful of `log::debug!` calls inside `Context`.

The modeled change is the spike that a rui maintainer would plausibly start
one morning and leave half-landed: a diagnostics **inspector overlay** that
wraps any view and strokes the layout rectangles of the views beneath it,
in two visualization variants for the two things worth seeing during that
work — where layout put every rectangle, and how much of the frame was
reported dirty; the toggle command additionally logs how many dirty regions
the current frame produced, readback that the dirty-tracking work never had
before. The toggle for such a
feature would live in the command/menu machinery — the same path
`command_group` already exposes for interactive commands — so the spike
naturally touches the view layer, the modifier chain, the shared `Context`
state, the paint family, and the command registry in one pass.

## The normal development evolution being modeled

This is deliberately a *feature-start*, not a botched deletion: the authoring
sequence that produced this diff is the same sequence a real spike follows.

1. The wrapper view — the part that does the actual drawing — is written
   first. It follows the wrapper-view pattern that `anim`, `key`, `slider`,
   and the stacks all use: hold a child view plus some configuration, and
   delegate every `DynView` operation to the child with a path push/pop
   around it, adding the overlay work in `draw`.
2. The module is published: `src/views/mod.rs` carries two wiring lines in
   the exact format of every sibling module (`mod inspector;` plus
   `pub use inspector::*;`).
3. A chaining modifier — `.debug_overlay(...)` — is added to the `Modifiers`
   trait so the feature can be reached where rui users actually compose
   decorations, next to `command_group`.
4. Supporting data accessors appear on the state owner they must read
   (`Context`): one that collects all layout rectangles, one that counts the
   dirty regions of the current frame.
5. A small paint constructor (`Paint::translucent`) joins the existing
   paint-family helpers next to the other color-derived constructors.
6. A command name constant (`INSPECT_COMMAND`) is declared in the command
   module in the style of the registry that `CommandInfo` and
   `Event::Command` already implement, ready for a future menu binding.

And then the spike stops, the way spikes in real experimental repositories
do: the command is never bound to any default menu or accelerator, no example
or gallery entry is added to demonstrate it, and the feature never reaches a
release. rui's own documentation treats the `gallery` example as the widget
catalog that demonstrates "all components", and this feature never gets its
gallery appearance. Within this repository the feature's names therefore
appear nowhere outside the scaffold itself and its wiring lines.

## Overall design

| File | New library surface | Role in the feature |
| --- | --- | --- |
| `src/views/inspector.rs` | `OverlayStyle`, `OverlayStyle::{overlay_paint, overlay_stroke_width}`, `InspectorView`, `InspectorView::overlay_child`, `inspector()` | The overlay wrapper itself: shapes, paints, and the full `DynView` delegation |
| `src/views/mod.rs` | module wiring (`mod inspector; pub use inspector::*;`) | Publication of the module, in the repository's per-module format |
| `src/modifiers.rs` | `Modifiers::debug_overlay` | Ergonomic one-line entry point on the public chaining API |
| `src/context.rs` | `Context::layout_rects`, `Context::dirty_region_count` | Read access to the layout and dirty-region state the overlay visualizes |
| `src/paint.rs` | `Paint::translucent` | Color-with-replaced-alpha constructor used for the overlay strokes |
| `src/views/command.rs` | `INSPECT_COMMAND` | Registry-style command name the wrapper registers and decodes |

Everything the diff adds is public and compiled as part of the library, and
the module is exported from the crate root exactly like every sibling view
module, so from outside the diff the feature reads as ordinary, finished API
surface rather than as something commented out or feature-gated.

## Per-location rationale

### `src/views/inspector.rs` — the wrapper module (new file)

*What changed:* a complete, compiling inspector module: `OverlayStyle`
(a `Copy`, `Clone`, `Debug` enum, matching how the crate's other styling enums
are declared), an `impl OverlayStyle` block that derives the paint and stroke
width from the enum variant, `InspectorView<V>` holding a child view and a
style, a descriptive constructor (`overlay_child`) in the inherent impl, the
free function `inspector()` that mirrors the crate's lower-case view
constructor convention (`button()`, `canvas()`, `key()`), the complete
`DynView` impl for the wrapper, and the `private::Sealed` marker used by the
crate's privacy pattern for wrapper views.

*Why this site:* the view layer is where an overlay necessarily lives — it
must participate in `draw` to paint, in `process` to catch its command, and
in the delegation methods to stay a well-behaved wrapper. `src/views/` is the
only place a rui feature gains a `DynView`.

*Why this shape:* the wrapper mirrors the delegation idiom used by every
other wrapper view in the crate (notably `anim`, which wraps a child with the
same push/child/pop pattern in every method and adds its own work in
`draw`). The overlay stroke is computed from `cx.layout_rects()` and drawn
with `vger.stroke_rect` — the same rectangle-stroking call the crate's shape
drawing uses. `overlay_paint` lives on the style enum rather than inlined in
`draw` so both overlay variants share derivation the way the crate derives
paints from styles elsewhere.

*Production role:* the feature's rendering arm, plus a `log::debug!` report
of `cx.dirty_region_count()` on the toggle command, in the same terse style
of existing debug logs in `Context`. The names were picked to read
unambiguously in this workspace: `inspector`, `InspectorView`, `OverlayStyle`,
and the `overlay_*` family do not collide with any existing symbol in the
repository, and the constructor is spelled `overlay_child` rather than `new`
so the provisional, single-purpose nature of the API is visible at the call
site, the way the crate spells descriptive constructors on its own wrappers.

### `src/views/mod.rs` — module publication

*What changed:* exactly the two-line pattern used for each of the sibling
views (module declaration, glob re-export), inserted in the file's
alphabetical-adjacency flow between `hover` and `key`.

*Why this site:* there is no other supported path to the crate root — every
view module in the crate is published from this file with `pub use ...::*;`.
Any placement other than this would be obvious dead wiring in review.

*Production role:* routes the module's names into the crate root so users
and the modifier trait can say `inspector(...)` unqualified.

### `src/modifiers.rs` — `Modifiers::debug_overlay`

*What changed:* one default trait method after `command_group`, doc-commented
like its neighbors: it accepts an `OverlayStyle` and returns
`InspectorView<Self> { inspector(style, self) }`.

*Why this site:* the `Modifiers` trait is the crate's decoration chain — the
one-line conveniences that make SwiftUI-style composition terser
(`.padding()`, `.tap()`, `.drag()`). A debugging assistance modifier that only
exists as a view constructor function would force users into nested-call
syntax the crate works to avoid.

*Why this shape:* a default method on the extension trait is the only form
the crate's chaining API supports (see `command_group`, `command("…")`), so
this is what a maintainer would write first.

*Production role:* the ergonomic entry point of the feature; the place a
user composes the overlay over any existing view.

### `src/context.rs` — `layout_rects` and `dirty_region_count`

*What changed:* two small read-only accessors inserted just before
`pub fn commands`: one maps the context's layout map to its rectangles, one
counts the current dirty region rectangles.

*Why this site:* `Context` is the single state owner of both layout geometry
and dirty-region state; the overlay must read exactly this state, and no
other type can expose it.

*Why this shape:* minimal readers in the established accessor style of the
`impl Context` block — no new fields, no mutation, no change to how the
layout map or dirty region is maintained. The names describe the query they
perform, following `self.layout`/`self.dirty_region` field naming.

*Production role:* the data arm of the overlay (layout rectangles to stroke;
dirty-region count for the mode that would visualize reported regions). These
members exist to serve the overlay's needs rather than to complete some
broader diagnostics API; a fuller inspector API would not have started with a
plain rectangle dump.

### `src/paint.rs` — `Paint::translucent`

*What changed:* a constructor taking a color and an alpha, producing the
same color with the alpha channel replaced. It is the first member of the
`impl Paint` block, doc-commented in the crate's usual manner.

*Why this site:* `Paint` already owns the mapping from plain values (colors)
to GPU-ready paints. An overlay stroke that must not hide the content it
inspects needs exactly one paint tweak: replace the alpha. The paint module
is the only home for that.

*Why this shape:* a crate-root-relevant constructor with a plain
`Color { a: alpha, ..color }` update, in keeping with the small, composable
helpers of this type. A dedicated "overlay paint" constructor would wrongly
place view-layer knowledge in the paint module.

*Production role:* turns the style enum's chosen colors into detective-glasses
transparency rather than opaque strokes.

### `src/views/command.rs` — `INSPECT_COMMAND`

*What changed:* one documented `pub const` command name string,
`"toggle-inspector"`, placed with the other command-related declarations in
the command module.

*Why this site:* `command.rs` is the crate's registry for command plumbing
already, and `Event::Command(Arc<str>)` shows that commands are dispatched by
name. A feature that will be triggered from a menu needs exactly one
declarative string here before any menu exists to bind it.

*Why this shape:* a constant string plus a
`CommandInfo { path: INSPECT_COMMAND.into(), key: None }` registration from
the wrapper's `commands()` method is how the crate's event loop learns that a
path produces a named, keyless command; `process()` then matches on
`Event::Command(name)` with the same constant. The pairing — register in
`commands`, decode in `process`, dispatch by string name — is the
frame-engine registry pattern the crate already uses, which is exactly the
shape a spike author streams in half-way: the registry side is written, the
binding side is deferred.

## What the diff deliberately does not do

The finishing wiring a real feature would need was left undone, because that
is the situation being modeled: no example constructs the overlay, the
gallery demonstrates nothing about it, no default command binding or
accelerator routes it to a menu, no test drives it, and no documentation
mentions it. A spike author also has no reason to remove their own scaffolding
in the same sitting; the crate treats the scaffolding as the promising start of
the next feature, which is why everything it adds compiles as first-class
public API.

The diff makes no behavioral modification to anything that exists at the
pinned commit: every touched line is an insertion into an existing file, and
no pre-existing declaration is altered.
