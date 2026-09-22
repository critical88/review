# Injection design record — dead-code case in the modules lesson

## Maintenance motivation

The `modules` chapter of this teaching repository demonstrates, next to its
lesson texts, a tiny importable package: a mock "sound" package with a
sound-effects subpackage and a file-formats subpackage. Students read its
sources, and the packages lesson test imports it three different ways to show
the import syntaxes. Because students copy patterns from this package, it is
maintained the way a real small library would be.

The evolution this case models is the retirement of a dual-version support
layer — one of the most common sources of retained dead code in small Python
libraries. Earlier editions of the exercise notebooks ran on both major Python
versions, so the mock package carried the same kind of runtime compatibility
shim that popular frameworks of that era kept in a `_compat.py` module:
audio-protocol switches pinned at import time, byte-buffer based readers and
stream wrappers in a small legacy codecs module, and guard branches
throughout the package that routed work through the old machinery whenever one
of the switches admitted it.

When the notebooks were updated to the current interpreter only, the natural
first step happened: the switches were pinned — old protocols off, modern
buffer API on — and the notebooks went on working. The guarded branches were
left in the tree instead of being deleted, as they so often are, because the
"just turn the switch off" cleanup never reaches the code the switch was
guarding. The layer kept accreting small follow-ups of the same shape: import
aliases retained for "first edition scripts" at the package root, an AIF reader
that grew a mock return in front of its old form-chunk decoding path, aliases
kept at both subpackage roots for notebooks that no longer exist. Every one of
those regions is now provably unreachable under the configuration the sources
themselves ship, yet the package still reads as if the old interpreters were a
supported target a student might reasonably ask about.

## Overall design

The injection distributes one retained compatibility layer across the whole
mock audio package in `src/modules/sound_package`, so that the obsolescence
is not engineered as isolated snippets but as the residue a real cleanup would
have to trace across nine interacting files:

1. **the retired runtime configuration** — a switch module at the package root
   following the `_compat.py` convention: the Python-2 audio-protocol switch
   and the legacy-decoder switch pinned `False`, the modern-buffer switch
   pinned `True`, each with a docstring-style comment explaining which era
   read it (mirroring how the repo's lesson files comment their own history),
   plus its own never-executed guard demonstrating the switches in use;
2. **the legacy implementation** — a small codecs module of the kind that
   actually populated such shims: an `array`-buffer wave opener, a
   `struct`-header AIF chunk reader, a chunk decoder, a
   `StringIO`-based stream factory shim with the old try/except fallback, and
   a byte-repr helper, all written as plausible Python-2-era code rather than
   filler;
3. **guard-shaped retention** — every consumer of that layer sits behind an
   import-time condition keyed on the switches, in the three shapes such
   residue really takes: whole guarded suites at module scope (package root,
   both subpackage roots, the reverse effect module), a dead alternative arm
   inside an import-time decision (the buffer-model documentation at the
   package root, whose guard is true-or-true on every supported interpreter),
   and inlined legacy return paths inside the functions students read (the wav
   reader and the echo effect);
4. **simplification residue** — the AIF reader opens with the mock return the
   current lesson teaches and then keeps its old form-chunk decoding tail in
   the function body below it, the shape that is left behind when a real
   function is simplified without cleaning its old end.

Two style constraints were kept throughout: the added code matches the
repository's habits (lesson-flavored docstrings with the same `@see:` link
pattern the clean files already use, parenthetical history in training-style
comments, mock string returns), and no file outside the package is touched, so
the lesson texts, the tests, and every other lesson module stay byte-identical.

## Per-location rationale

### `sound_package/_compat.py` (new)

**What:** package-root compatibility module with three documented import-time
switches, plus its own small guard suite wiring the Python-2 `StringIO` module
into a stream factory while the protocols switch is enabled.
**Why this site:** the `flask`-era `_compat.py` convention makes a package-root
switch module the natural, recognizable home for this kind of layer; a switch
module is also the only way "supported configurations" can be an in-tree,
provable fact rather than an assumption. The docstring names the lesson
context and the framework precedent, so the module reads as deliberate lesson
coding, and the guarded suite inside it models the switches' original
consumers.
**Production role:** the retired configuration surface itself — the constants
the retained branches throughout the package key on.

### `sound_package/_codecs_py2.py` (new)

**What:** the legacy implementation: five small functions in the idiom of
Python-2 audio handling — an `array` buffer "c"-typed file opener, a
`struct.unpack` loop over AIF form chunks with big-endian `>H` headers, a chunk
decoder, a `StringIO` stream factory with the old try/except import fallback
(the module was genuinely importable on both versions once), and a byte-oriented
repr helper.
**Why this site and this shape:** the layer needs a plausible legacy
implementation, and one that a maintainer could plausibly have written in this
era: era-accurate idioms (`array`, `struct`, `io` fallbacks) are what
distinguish retained-but-obsolete code from filler surprisingly well.
Functions are separated the way a shim would actually organize them
(reader helpers vs stream/repr helpers), so different call sites have different
favorites, which is what a real cleanup path has to disentangle.
**Production role:** the retained implementation artifact — the machinery that
every guarded region would route through.

### `sound_package/__init__.py`

**What:** turns the empty package initializer (the clean tree ships it empty —
the packages lesson itself explains that an empty `__init__.py` is the simplest
valid form) into a documented package root: a docstring in the clean files'
voice, live interpreter-version and switch imports, an import-time
"buffer-model documentation" decision, and a guard suite keeping the
first-edition root aliases (`wave_open`, `chunk_read`) alive behind the
protocol switch.
**Why this site:** the buffer-model decision models the
constant-true-guard residue pattern — one arm documents the supported model
while the other arm, kept for older interpreters, would re-home the legacy
decoder; a maintainer sees this shape whenever a dual-path decision is left
after one path retires. The alias suite models the package-root convenience
export, the classic thing a "first edition scripts still import this"
comment keeps in the tree long past its public.
**Production role:** the package's import-time surface — the file every
importer of `sound_package` executes first.

### `sound_package/effects/__init__.py`

**What:** documents the subpackage, then keeps a guard suite for the
Python 2.7 iterator-protocol wrappers: legacy echo-repr and stream-wrapper
aliases keyed on the protocol switch and an exact `(2, 7)` version check.
**Why this site:** a subpackage root carrying a small block of import-time
aliases is exactly where such layers accumulate, and the empty-to-documented
evolution mirrors how real packages grow roots around their leaves. The
`sys.version_info[:2] == (2, 7)` guard shows the version-pinning idiom a
real-era change would contain (guarding on a tuple slice), not a synthetic
condition.
**Production role:** import-time initialization of the effects subpackage.

### `sound_package/effects/echo.py`

**What:** the live echo effect gains an in-function branch routing through the
legacy repr helper while either switch admits it, in front of the unchanged
mock return.
**Why this site and this shape:** effects are the leaf functions students
read first, so an inlined legacy path here is where the maintained-vs-obsolete
boundary becomes visible in behavior-carrying code; routing through
`py2_audio_repr('Do echo effect')` means the retired path, if ever reactivated,
changes the exercise's expected outputs — and its guard is a two-switch
disjunction, the natural "while either legacy mode is enabled" condition.
**Production role:** the echo effect exercise function the packages lesson
test calls.

### `sound_package/effects/reverse.py`

**What:** the reverse effect module keeps a module-level guard suite binding
its legacy stream-wrapper alias behind the protocol switch; `reverse_function`
itself is untouched and keeps returning the plain mock string.
**Why this site:** module-level guard residue is a distinct shape from the
in-function branch (it is evaluated at import time, not at call time), and the
reverse effect — a function no other module in the clean tree ever imports —
is the kind of quiet corner where a cleanup's reach gets tested against the
"uncalled ≠ obsolete" distinction: the module is legitimately part of the
lesson, but this guard is provably inert.
**Production role:** the reverse effect exercise module.

### `sound_package/formats/__init__.py`

**What:** documents the formats subpackage, then keeps a guard suite
registering legacy reader aliases (`READER_ALIASES` mapping container keys to
the legacy chunk reader and the 'array' buffer model) behind the decoder switch
and a below-3 interpreter check.
**Why this site:** the alias-table register is the third classic retention
shape — a registry built at import time for consumers that no longer exist —
and sits naturally beside the subpackage's reader modules. The inverted
version comparison (`< 3`) gives the layer another era-accurate guard idiom.
**Production role:** import-time initialization of the formats subpackage.

### `sound_package/formats/wav.py`

**What:** the wav reader gains an in-function fallback branch that opens the
lesson file through the legacy `array`-buffer opener when the modern buffer
API is unavailable, returning the open buffer in place of the mock string.
**Why this site and this shape:** a reader that returns a buffer under the old
model is the historically correct shape of this era's compat code (the return
type changed with the buffer model, which is exactly why the guard is a
negated switch), and the wav reader — like reverse — is caller-less inside the
clean tree, so the case exercises the distinction that a function nobody
currently imports can still be a real lesson artifact while its old-model
extra path is provably inert.
**Production role:** the WAV reader exercise function.

### `sound_package/formats/aif.py`

**What:** the AIF reader keeps its documented mock return and, below it in the
same function body, its old form-chunk decoding tail — import of the legacy
chunk reader, a byte-string form-header sample, a conditional legacy return.
**Why this site and this shape:** simplification residue after a function's
contract is rewritten (here, becoming a mock for the current lesson text) is
the least searchable form of retained dead code: nothing keys on a switch, the
guard after the return depends on data the tail itself produces, and reading
the function top-to-bottom is the only way to notice the return flushes
everything after it. Including one region of this shape keeps the case honest
about how inconsistently real dead code hangs together.
**Production role:** the AIF reader exercise function.
