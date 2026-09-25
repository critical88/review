# Injection design record — value-contract and input-stream interfaces in jaq

This record documents the design of the changes that turn the pinned clean
checkout into the injected case. It is written as auditable design rationale.

## Maintenance motivation

The jaq ecosystem splits its crates by layer: `jaq-core` holds the parser,
compiler, and interpreter together with the minimal value trait (`ValT`) that
every value type must implement; `jaq-std` layers the standard library on top,
including a wider value trait whose string/number/sequence capabilities the
native std filters rely on; `jaq-json` provides the concrete JSON value type
used by the `jaq` CLI and most embedders. Downstream users repeatedly ask for
the same two conveniences:

1. *One trait to implement.* Embedders implementing their own value type are
   surprised that satisfying the core trait is not enough to run standard
   filters: they must also implement the std extension trait, and the exact
   split between the two is not documented anywhere prominent. A maintainer
   under that pressure plausibly decides that the split is accidental and that
   "the complete value contract" belongs in one place, so a value type gets
   the whole standard library for free by implementing a single trait. The
   ecosystem-facing re-export keeps every existing import compiling while the
   contracts are unified.

2. *Core self-sufficiency for inputs.* The `inputs`/`input` filters read from
   an input stream that lives in the filter execution's global data. Upstream,
   that capability lives in the std crate and is opt-in: a data kind declares
   it, and the std filters demand it via bounded trait methods. A maintainer
   reasoning that "the input stream is part of the global data a filter
   execution has access to — just like its lookup table" plausibly moves the
   machinery into core, makes it part of the data-kind contract, and removes
   the `where` clauses that threaded the capability through the std filters
   ("there is no data kind without an input stream"). Data kinds that were
   historically input-free — a bare LUT — are then bridges with a degenerate
   empty stream implementation.

## Modeled development evolution

The changes model one ordinary-looking refactor commit from a maintainer with
best intentions and deadline pressure, not a malicious edit: both clusters
simplify some *usage* at the cost of interface discipline, in a way that keeps
every existing import path resolving and every visible behavior intact.

## Overall design

The injection touches three production layers and seven files:

- the core value-contract module (`jaq-core/src/val.rs`) and its facade
  (`jaq-core/src/lib.rs`),
- the core data-kind contract (`jaq-core/src/data.rs`) plus a new core input
  module (`jaq-core/src/input.rs`),
- the std extension surface (`jaq-std/src/lib.rs`, `jaq-std/src/input.rs`) and
  the JSON value implementation (`jaq-json/src/lib.rs`).

## Cluster A — merge the std value extension into the core value contract

**What changed.** The eight required capability methods of the std trait
(`into_seq`, `is_int`, `as_isize`, `as_f64`, `is_utf8_str`, `as_bytes`,
`as_sub_str`, `from_utf8_bytes`) are hoisted into `ValT` in
`jaq-core/src/val.rs`, together with three provided convenience helpers built
on them (`as_utf8_bytes`, `try_as_bytes`, `try_as_utf8_bytes`). The core
trait's supertraits grow `Ord` and `From<f64>`, which generic sort and
number-conversion paths in the standard library previously obtained via the
std-only extension trait. `ValT` in `jaq-std/src/lib.rs` stops being a defined
trait and becomes a plain re-export of the core trait, and the now-redundant
private `fail_str` helper disappears in favor of the core-provided
`try_as_bytes`/`try_as_utf8_bytes` helpers. `jaq-json/src/lib.rs` merges both
of its trait implementations into a single implementation of the unified core
trait.

**Why this site and this shape.** `ValT` is the ecosystem-wide choke point:
every native core filter is generic over it, and the only in-tree value
implementor is the JSON value. Merging here therefore produces the widest
realistic contract in the codebase while the tree still compiles cleanly — the
JSON value already implements every method, it now just implements them for
the core trait directly. The re-export is what makes the change look
attractive in review: `use jaq_std::ValT` keeps resolving, so nothing in the
ecosystem breaks at compile time. The cost is paid by every *future or
external* implementor of the core trait and by every data kind that has no use
for string/number capabilities.

**Production role.** This is the gate through which a value type becomes
usable by the interpreter; its method list is the de-facto compatibility
contract for embedders.

## Cluster B — fold the input-stream capability into the core data contract

**What changed.** A new module `jaq-core/src/input.rs` takes ownership of the
input machinery (`Inputs`, `RcIter`, and the `HasInputs` capability trait)
that previously lived in `jaq-std/src/input.rs`; the std module shrinks to a
thin re-export plus the two native filters. The associated `Data<'a>` type of
the core data-kind trait (`jaq-core/src/data.rs`) gains a `HasInputs` bound, so
the input-stream capability becomes part of what every data kind must provide.
The bare-LUT fallback (`&Lut<JustLut<V>>`), which historically carried no
inputs, bridges the new mandatory bound with a degenerate implementation that
leaks an empty input stream. The `where for<'a> D::Data<'a>: HasInputs<...>`
clauses on the std filters are deleted — the point of the fold is that core
now guarantees the capability.

**Why this site and this shape.** The data-kind trait is where compile-time
and run-time data meet, and its associated `Data` type is the natural place a
maintainer extends "global data". Making the capability mandatory changes the
default for every data kind at once instead of threading bounds through
callers — a real simplification of the std filter signatures — and the
degenerate empty-stream implementation is small, local, and easy to accept in
review because it is only exercised by kinds that never read inputs.

**Production role.** This plumbing decides what a filter execution may read
besides its lookup table; the capability boundary between "data kinds with an
input stream" and "data kinds without one" is what the change erases.

## Sites considered and not used

Several neighboring candidates were evaluated during design and left alone
because they would not carry the modeled relation plausibly:

- The loader's AST trait and the compile-time `Expr` machinery are internal
  single-purpose contracts; widening them would not correspond to any
  realistic embedder request.
- The path/update traits sit behind the core trait and have narrow, well-used
  surfaces; nothing realistic is gained by widening them.
- The LUT-access trait `HasLut` is a one-method capability that every core
  filter genuinely uses; it could not plausibly be accused of forcing unused
  capability on anyone.
- Adding brand-new marker or capability traits to manufacture interfaces was
  rejected outright: the codebase's history shows contracts emerging from the
  std layer as capabilities accumulate, which is exactly the seam the two
  clusters above exploit in reverse.

## Behavior preservation strategy

The reorganization moves method bodies between traits and keeps the native
filter implementations calling the same underlying operations on the same
value types; filter semantics, CLI behavior, and the publicly documented
import paths are meant to be indistinguishable from before for every existing
consumer. No value-construction, parsing, or execution path is reimplemented —
the intent is purely a regrouping of where capability requirements are
declared, which is precisely why this kind of change survives review.
