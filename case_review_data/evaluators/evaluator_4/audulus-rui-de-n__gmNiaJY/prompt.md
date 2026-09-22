# Clear out the abandoned layout-diagnostics scaffolding in rui

## What you're working with

This repository is `rui`, an experimental, SwiftUI-style UI library for Rust
(version 0.6.1 at this state of the tree). The whole library is GPU-rendered
and reactive: there is no retained view tree — views are lightweight values
that implement a delegation trait, and `Context` is the single state owner
that stores layout rectangles, dirty regions, user state, and bookkeeping for
every view. Public views are small modules under `src/views/` re-exported from
the crate root, `src/modifiers.rs` provides the chainable convenience trait
(`.padding()`, `.tap()`, …), commands are dispatched by string name through
`Event::Command`, and examples under `examples/` double as the project's demos
and integration surface (the gallery example demonstrates the widget
catalog).

Work happens with plain cargo, on the stable toolchain the repository pins:

```bash
cargo build                              # library and binary targets
cargo build --examples                   # the demo/integration examples
cargo test --lib -- --skip test_screenshot_   # headless library suite
```

The screenshot regression tests need a GPU and are skipped in this
environment; the rest of the library suite runs headless and is green on the
tree you are starting from.

## The observation

While doing a tidy-up pass over our public API before the next polish round,
I ran into the leftovers of a layout-debugging feature that someone clearly
started during earlier layout-engine work and never finished: a wrapper view
that is supposed to draw a semi-transparent diagnostics overlay — the layout
rectangles of the views underneath, with a dirty-region reporting variant —
over the view it wraps. The scaffolding for it is real and it compiles: the
module is wired into the crate root exactly like every other view module, it
even grew a one-line modifier on the convenience chaining API so it could be
reached the way users reach decorations, and there is a command name checked
in beside the command plumbing for a toggle that was obviously meant to be
bound to a menu or accelerator someday.

What there is none of is any sign of use. No example constructs it. The
gallery never demonstrates it. Nothing binds its command. No test exercises
it. And because everything involved is `pub` and reaches the crate root
through re-exports, the compiler will never complain about any of it — rustc
does not warn on reachable public items — so the whole thing just sits there
presenting itself as finished API surface that every future refactor has to
drag along.

We've decided not to finish this feature now; we want the half-built
scaffolding gone.

## What to do

Treat the feature as abandoned and remove all of it:

- Remove the overlay wrapper itself, including its module wiring, the
  modifier shortcut that was added for it, and the command name that was
  reserved for it.
- Hunt down and remove every small supporting member that was added elsewhere
  in the library to serve this feature and that nothing else uses — helpers,
  accessors, constructors, constants. These are easy to miss: once the wrapper
  that called them is deleted, the compiler gives no further hint about them,
  so check deliberately what the feature's own code referenced, and who else
  (if anyone — library code or examples) references those members today.
- Do not finish or demo the feature instead; the outcome we want is the
  cleaned-up crate, not a finished debugging feature.

Be precise about the boundary, in both directions:

- **Everything that predates this feature stays exactly as it is.** This
  crate already carries a few long-dormant helpers of its own that nobody has
  used in a while. They are not part of this task. Do not remove, rename, or
  rewrite any API item that already existed at the current tree's base state.
- **No dangling references anywhere.** After your removals, the library, every
  example, and the test suite must all build and pass just as before. If a
  removal would leave anything incomplete or broken, keep the removal set
  consistent rather than hoping a warning is harmless.

## What must remain stable

- All pre-existing public API items: names, signatures, and semantics
  unchanged.
- The rendering, layout, dirty-tracking, event-dispatch, accessibility, and
  GC behavior of every existing view and modifier — exactly as before.
- The green state of the build and the library test suite (no new failures).

## How to convince yourself you're done

- The crate-root API surface (module exports, the chaining trait, command
  constants, helper constructors, state-owner accessors) contains no trace of
  the half-built feature, and a workspace-wide name search for what it used
  to be called finds nothing outside your own deletions.
- `cargo build`, `cargo build --examples`, and
  `cargo test --lib -- --skip test_screenshot_` all succeed.
- Your diff only deletes code that this abandoned feature introduced; nothing
  pre-existing is touched other than the plumbing that the feature's own
  additions required.
