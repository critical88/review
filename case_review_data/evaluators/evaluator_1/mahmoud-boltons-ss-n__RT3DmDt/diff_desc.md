# Injection design record: scattered identity-sentinel provisioning in boltons

## Maintenance motivation

boltons is a "grab bag" utility library of roughly thirty modules, and one
recurring internal idiom crosses almost all of them: **identity sentinels**
(distinct falsy marker objects used to mean "argument not passed", "value
not supplied", "entry lazily deleted", or "walk finished" — situations the
caller cannot express with `None`, `0`, or `''`, because those are values a
caller could legitimately pass).

In the pinned state, every module that needs such a marker acquires it the
same way:

```python
try:
    from .typeutils import make_sentinel
    _MISSING = make_sentinel(var_name='_MISSING')
except ImportError:
    _MISSING = object()
```

The `try/except ImportError` fallback is a vestige of boltons' history as a
loose collection of standalone modules, imported individually before the
package layout settled. It has been effectively dead for years: the
`except` branch only triggers for split/partial imports that no longer
happen, and when it does trigger it silently downgrades markers to featureless
`object()` instances with address-based reprs and no copy/pickle behavior.

The realistic maintenance pressure here is twofold:

1. **Self-containment instinct.** boltons modules take pride in standing
   alone (see `queueutils`, which falls back to `list` if `listutils.BList`
   is unavailable, or the `threading` fallbacks sprinkled around the
   package). Individual module maintainers repeatedly feel the shared
   import is "too much machinery for what I need" — `make_sentinel` drags in
   frame inspection for pickleability and a heavier class than some
   modules want — and each one, in their own module, replaces the shared
   call with a tiny local constructor tuned to that module's needs.

2. **Cross-cutting policy drift.** Because each replacement is written for
   one module's local reason (memory in one, pickleability in another, a
   debug description in a third), the copies diverge in behavior. Once
   they exist, *any* library-wide decision about markers — what their repr
   convention should be, whether they pickle, whether copying returns the
   same object, whether names are validated — has to be re-made, and
   re-tested, in every module that owns a copy.

## Development evolution being modeled

The injection models the ordinary, incremental drift that produces this
kind of scatter: a series of small, individually-justified
"make this module stand alone / tune this marker for this module" touches,
each landing in a different module and at a different time, written against
that module's own local needs rather than against a shared plan. Every
fragment looks locally reasonable — most carry a comment explaining the
module-specific benefit — and no fragment ever *says* "I am forking the
shared responsibility"; the fork is only visible from the outside, when a
maintainer tries to change marker behavior once and discovers they must
touch a dozen files.

The change preserves observable behavior throughout: module-level marker
names, `is`-comparison semantics, falsiness, the printed marker reprs that
appear in documentation examples, copy/pickling behavior where reachable,
and all consumer code paths of the markers are unchanged. Only the
provisioning machinery itself is re-attributed from the shared owner to each
module.

## Overall injection design

The design targets one conceptual responsibility: **"create the
library's falsy identity sentinels."** In the pinned state that responsibility
has exactly one implementation (`boltons/typeutils.make_sentinel`) consumed
by every module through a uniform import block; the diff replaces that
uniform block in eleven participating modules with eleven locally-owned,
deliberately non-uniform implementations of the same machinery. The shared
owner remains in the package (it is public, documented, and directly
exercised by its own tests) but is no longer consumed by the sibling
modules.

Each module's fragment:

- removes the `try/except` acquisition block,
- defines private machinery that constructs that module's markers (the same
  marker names as before, still bound at module scope so every existing
  consumer is untouched),
- attaches a module-local rationale in a comment or docstring explaining
  why *this* module's variant is shaped the way it is,
- and varies structurally from its siblings, so the eleven copies reflect
  eleven local policy decisions rather than one pasted snippet.

Marker use sites are deliberately **not** modified: every `is _MISSING`,
`is _UNSET`, `is _REMOVED`, `is NO_DEFAULT`, `is _REMAP_EXIT`,
`is _KWARG_MARK` check and every default-argument binding keeps pointing at
the same module-level names. This keeps the change behavior-preserving and
keeps the diff one coherent provisioning-side story.

## The eleven locations and their selected shapes

The fragment shapes are intentionally varied: a uniform paste would be
ordinary duplication, whereas the modeled evolution is one of *divergence*
— same responsibility, drifting implementations. The diversity is
deliberate structural variation and each variant is documented below with
its chosen location, form, and production role.

### `boltons/listutils.py` — factory with a `__slots__`-tuned nested class

Replaced the import block with a local `_make_sentinel(var_name)` factory
that builds a closure-scoped `_Sentinel` class using `__slots__`. Chosen
because `BarrelList` is the package's most memory-sensitive list structure
(a comment says exactly that); a slots-based marker is the minimal-overhead
variant a perf-conscious maintainer of this module would write. Produces
`_MISSING` (default-missing marker for list arguments).

### `boltons/cacheutils.py` — memoizing factory with a registry, plus pickle policy

Caching argument markers are identity-stable by construction (cache keys
embed them), so this fragment adds a `_SENTINEL_CACHE` registry keyed by
name and a `__reduce__` that resolves through the module global — the
heaviest policy layer of the eleven, and the only one whose repr is
**published in a documentation example** (the boundary marker printed by the
cache-key builder's example output). Produces `_MISSING` and `_KWARG_MARK`.

### `boltons/funcutils.py` — bare module-level class, no factory

Instead of a factory, `_NoDefaultType` is defined at module level and
instantiated once as `NO_DEFAULT`. Selected to show the other common shape
of the same drift: when only one marker is needed, maintainers skip the
factory indirection entirely. `NO_DEFAULT` is the module's documented "this
argument has no default" marker consumed by the function-builder machinery,
so the class carries a docstring tying it to signature manipulation.

### `boltons/dictutils.py` — factory carrying copy/deepcopy protocol

`_make_sentinel(name)` builds `_MissingMarker` with `__copy__` and
`__deepcopy__` returning `self`. The rationale is local and real: this
module's containers (`OMD`, subdicts, one-to-one mappings) copy and
deep-copy user values, so a marker could otherwise be duplicated inside
copied structures and betray the `is`-based default checks. `_MISSING`
fronts ten-odd accessor/pop/setdefault defaults across `MultiDict`/`OMD`.

### `boltons/debugutils.py` — module-level picklable trace marker

`_UnsetType` with a `__slots__` layout and `__reduce__` returning the
module-global name; the docstring claims trace results can be shipped to
worker processes, so the marker must survive pickling. Produces `_UNSET`,
the "no return value captured" placeholder used by the trace-wrapping
helpers. A bare class (like funcutils) but with a serialization policy of
its own — a different local decision than its structural twin.

### `boltons/queueutils.py` — validating tombstone factory

`_make_tombstone(name)` is the only fragment that *branches*: it rejects
names that are not private identifiers before constructing `_Tombstone`.
The local justification is that lazy-deletion tombstones outlive many heap
operations and appear in tracebacks. Produces `_REMOVED`, consumed by the
priority-queue lazy-deletion protocol. This variant encodes the
"my module takes its policy seriously" flavor of drift.

### `boltons/iterutils.py` — class-name repr fallback

`_make_sentinel(name)` builds `_Marker` with a repr of the shape
`_Marker('_UNSET')` (class name plus quoted name) and no variable-name or
pickle behavior. This matches the module's pre-existing usage form
(positional name, no var_name) and its docstring explains the markers are
threaded through arbitrary user collections during `remap` walks. Produces
the two internal walk-state markers `_UNSET` and `_REMAP_EXIT`.

### `boltons/setutils.py` — dynamically assembled marker type

`_make_sentinel(name)` builds the marker class ad hoc with
`type('_Marker', (), marker_methods)` where the methods dict holds small
lambdas. The comment ties this to `IndexedSet`'s dead-slot tombstones
scattered across `item_list` — the most "clever local variant" of the
eleven, the kind that looks self-contained precisely because the class
object never enters the module namespace. Produces `_MISSING`, sampled
directly by the module's published tests and compared by identity.

### `boltons/socketutils.py` — two-argument factory mirroring the shared owner's signature

`_make_sentinel(name='_UNSET', var_name=None)` copies the shared owner's
exact signature (with the same defaulting rule), documented locally for
distinguishing an omitted *timeout*/*maxsize* from an explicit `None`.
Illustrates the near-copy variant of drift: a maintainer who ported the
shared function wholesale but kept it private, so future shared changes no
longer reach it. Produces `_UNSET`, consumed by every recv/send default in
`BufferedSocket`.

### `boltons/tableutils.py` — self-documenting bare class

`_MissingCellType` instantiated directly as `_MISSING`, the
"header/cell not supplied" marker used by the table constructors and
`from_*` loaders. Distinguished from its funcutils twin by carrying a
docstring-oriented identity (printable name for tables surfaced through
str/repr/exports) rather than a slots layout — a third bare-class flavor
with its own declared contract.

### `boltons/urlutils.py` — description-carrying factory

`_make_sentinel(name, description=None)` attaches free-form metadata to the
marker (`_QueryMarker` keeps a short description the parser could quote in
missing-field diagnostics). This is the only fragment that *extends* the
marker's data model rather than its protocol — the local "improvement"
that makes divergent copies sticky: once one module's markers carry
descriptions, the others look deficient. Produces `_MISSING`, the omitted-
vs-empty value marker in query-parameter accessors.

## Explicitly out of the scatter

Several similar-looking neighbors were evaluated and deliberately left
untouched so the boundary stays crisp and honest:

- `boltons/formatutils.py` binds its `_UNSET` as a bare `object()` at module
  scope, and `boltons/funcutils.py` binds a function-local `_UNSET = object()`
  inside a lazy-init wrapper. Both are sentinel *uses*, composed of no
  machinery and no policy; they do not implement creation behavior.
- `boltons/typeutils.py` continues to own the public, documented
  implementation of the conceptual responsibility (this function is directly
  exercised by the package's own tests and referenced by its docstrings); a
  single owner implementing the responsibility is the design the injection
  departs from.
- The `threading`/`BList` `try/except ImportError` fallbacks elsewhere in the
  package share the *import* idiom being removed, but own a different
  responsibility (import compatibility) and no marker behavior.
- `ioutils`/`setutils` classes defining truthy `__bool__` are container
  semantics, unrelated to identity markers.

The repository has no other sentinel provisioning sites: the modules
touched are exactly the modules whose code constructs or acquires identity
markers through the shared import. Modules without marker needs were not
given new needs, which would constitute padding.
