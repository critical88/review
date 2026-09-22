# Injection design record — retired machine-readable export surface

## Maintenance motivation

`lsd` renders listings for humans: grid, tree and one-line layouts styled with
icons and colors. At some point a complementary *machine-readable* export of a
listing was prototyped — a stream of plain rows that scripts and other tools
could consume without scraping terminal layout. The experiment was merged
behind per-channel rollout switches so integrators could dogfood it on a
pilot platform before a general rollout.

The modeled reality: the pilot never led to a general rollout. The external
consumers never confirmed the format, the rollout was left switched off, and
follow-up development (icon handling, sorter changes, CI modernization) went on
around the pipeline without ever revisiting the export. What remains is a
complete feature — entry gate, rendering sink, icon variant, ordering,
metadata formatting — that compiles in every build, references live members
and is itself referenced by live functions, yet can never execute. Nothing
alerts on it: the members are still textually referenced, so ordinary
unused-code detection stays quiet, and every activation path sits behind
switches that read off.

The diff models the repository at the end of that evolution. This file records
what was added and why each site was chosen; it deliberately does not claim a
verdict on what the additions mean or what else the tree contains.

## Overall design

The additions are a single feature's remains woven through the listing
pipeline that the feature served, rather than one isolated block:

- an orchestration-side activation gate plus summary builder in the core
  runner (the module that fetches, sorts and displays entries);
- a consumer abstraction with two buffer implementations, a boxed selection
  constructor and row helpers in the grid renderer, plus a render-time hook
  guarded by a depth ceiling;
- a pinned-glyph fallback chain in the icon lookup;
- a stable comparator beside the user-facing sorters;
- machine-formatting accessors on the entry metadata type.

Node count and the doc-comment voice were chosen to read like a feature that
was *finished and shelved*, not scaffolding: comments explain rollout staging,
integration conventions (`legacy-` prefixes, constant-width rows) and the
retirement timeline thread, the way a real feature that lost its owner does.

## Cluster: `src/core.rs` — activation gate and summary builder

Added `SCRIPTING_EMIT_ROLLOUT` (module-level `bool`, initializer `false`,
doc-comment describing staged per-channel rollout) and, in the listing flow of
`Core::run`, a guarded emission after `self.display(&meta_list)`:

```rust
if SCRIPTING_EMIT_ROLLOUT {
    let summary = self.emit_scripting_summary(&meta_list);
    print_output!("{}", summary);
}
```

plus `Core::emit_scripting_summary`, which orders the entries with the export
comparator, assembles the body through the renderer's row-stream helpers, and
prefixes each row with a glyph from the icon fallback chain.

This site was chosen because `Core::run` is the one method every completed
listing passes through; an export emitted next to the rendered output is
exactly where a real integrator-facing feature would hook in. A named rollout
constant rather than an inline `false` mirrors how staged releases are
actually wired, and it keeps the guard looking configuration-driven even
though this channel's value is off. The summary builder produces the role the
rest of the additions serve: without it, the renderer/icon/sort/meta members
would have no plausible reason to exist.

## Cluster: `src/display.rs` — row-stream sink family and render hook

Added, above the existing rendering functions:

- `LEGACY_SCRIPTING_DEPTH_CEILING` (`usize`, value `0`), documented as the
  depth the integrations were sized against;
- `pub(crate) trait ScriptRowSink { fn push_row(&mut self, row: &str); fn finish(&self) -> String; }`;
- `LegacyPipeSink` (rows joined with `\n`, matching the pipe integration) and
  `LegacyBufferSink` (rows joined with a space, matching the clipboard
  scripts), both plain `Vec<String>` buffers;
- `legacy_sink(row_estimate) -> Box<dyn ScriptRowSink>`, a registry-style
  constructor that picks the pipe layout for even row counts and the buffer
  layout otherwise;
- `scripting_rows(&[&Meta], &mut dyn ScriptRowSink)`, which pushes each entry's
  rendered row through the sink;
- `legacy_render_row(&Meta) -> String`, the machine-oriented row formatter.

And inside `inner_display_grid`, after the existing columns are fitted and
before the folder-path decision, a hook is guarded by
`depth < LEGACY_SCRIPTING_DEPTH_CEILING`:

```rust
if depth < LEGACY_SCRIPTING_DEPTH_CEILING {
    let ordered: Vec<&Meta> = metas.iter().collect();
    let mut sink = legacy_sink(ordered.len());
    scripting_rows(&ordered, sink.as_mut());
    output += &sink.finish();
}
```

This cluster carries the design's main dynamic-dispatch disguise. The sink is
reached through `Box<dyn ScriptRowSink>`, so the two implementations never
appear at their textual call sites, and the parity-based constructor makes the
choice look data-dependent. The same file holds the second guard grammar
deliberately: an unsigned comparison against a zero constant
(`depth < LEGACY_SCRIPTING_DEPTH_CEILING`) instead of the boolean rollout
constant, so the retired surface is not recognizable by one pattern, and the
hook reads like "the integrations only ever consumed the top level" — false
for every call, since `depth` starts at zero and unsigned types cannot go
below the ceiling.

The renderer was chosen as the second-largest cluster because it is where
listing content is serialized into consumer-visible bytes; an export surface
that bypasses human styling but reuses entry metadata naturally lives beside
the grid code it once mirrored.

## Cluster: `src/icon.rs` — pinned-glyph fallback chain

Added `SCRIPTING_ICON_FALLBACK` (`bool`, initializer `false`, documented as
keeping export rows constant-width across icon themes) and, at the top of
`Icons::get` — the method every icon lookup funnels through — a guarded
shortcut:

```rust
if SCRIPTING_ICON_FALLBACK {
    if let Some(glyph) = self.scripting_glyph(&name.name) {
        return format!("{}{}", glyph, self.icon_separator);
    }
}
```

followed by `Icons::scripting_glyph` (a `legacy-` name override, else a fixed
fallback glyph), `Icons::legacy_glyph_override`, and the module-level
`legacy_fallback_glyph()` (`"\u{f4a1}"`).

The icon module was included because a machine format that must survive
diffing cannot tolerate theme-dependent glyphs, so a real export feature would
have pinned its own glyph here; the chain also places one cluster head on a
method that is genuinely hot at runtime. All three members are reached only
from export code (`Icons::get`'s never-taken branch and the summary builder),
so the live theme resolution below the guard is untouched.

## Cluster: `src/sort.rs` — export comparator beside the live sorters

Added `by_scripting_path` and its key builder `scripting_path_key` (a
`path + '\0' + name` composite that orders machine rows stably and
independently of the user's flag-driven sorter assembly), placed directly
after `by_git_status` among the other comparators.

Only the summary builder references this comparator, which models the real
property that the export needed a fixed order while users rebind theirs with
flags. Its placement inside the sorter module means the retiree sits
textually adjacent to near-identical live code — the sorters must be
understood individually, not by shape.

## Cluster: `src/meta/mod.rs` — machine-formatting accessors

Added `Meta::scripting_name` (entry name, an em-space, then the path-suffix
label) and `Meta::legacy_relative_label` (canonicalized path string when
resolvable, raw path otherwise), closing the `impl Meta` block.

The metadata type is the pipeline's authority on entry identity, so
machine-oriented name/label assembly belongs on it; the canonicalize-else-raw
fallback mimics real-world path robustness work. Both accessors are consumed
only by the row formatter, making this the deepest hop from the activation
gates (core → renderer row helper → metadata accessor → label helper).

## Reachability shape of the additions

Every chain starts at one of the three compile-time-off branches:

- core gate → summary builder → comparator/key pair (sort module);
- core gate → summary builder → sink constructor/driver → trait `push_row`
  (dyn-dispatched to either buffer implementation) and row formatter →
  metadata accessors;
- core gate → summary builder → icon glyph chain (method, override,
  fallback);
- render hook (renderer) → the same sink family from the rendering side;
- icon gate → icon glyph chain from the live lookup method's perspective.

## Behavior-preservation choices built into the design

- The diff is pure additions (175 insertions, no deletions): no existing
  statement, signature or constant is modified, only appended-to contexts.
- All added branches are decided by constants fixed at compile time, so the
  added code paths cannot change any executed instruction sequence; with the
  gates off, `Icons::get`, `inner_display_grid` and `Core::run` behave exactly
  as before.
- Every addition is `pub(crate)` or private; no crate-visible or CLI surface
  changes.
- Nothing is added inside `#[cfg(test)]` modules or any integration test
  directory: testing scope is left as the pinned revision provides it.
- No existing `use` statement is altered; added call sites use
  module-qualified paths.
