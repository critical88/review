# Injection design record — kpatch-build pipeline consolidation

## Realistic maintenance motivation

The `kpatch-build` userspace toolchain is a set of pipeline stages wired
together by a shared ELF container (`struct kpatch_elf`). Historically each
unit step — correlate one section pair, compare one correlated section, look
up a symbol in one scope, remove and free one section, allocate a section
pair, rebuild one rela section buffer — lived in its own small helper, and the
three binaries (create-diff-object, create-klp-module, create-kpatch-module)
sequenced those helpers from a handful of driver functions.

During sustained feature work (Clang anonymous sections, `.sframe` and
patchable-function-entries support, the unified `.kpatch` to `.klp` format
integration) a developer under review pressure found it easier to "open a
stage up" in its driver than to thread new details through the shared
helpers: the new logic talks to loop variables and intermediate structures the
helper does not receive, so pasting the helper's body next to the loop avoids
inventing new parameters, flags, or return structs in kpatch-elf.c and
lookup.c — files whose helpers every binary links against and that therefore
cannot be churned casually.

Each such paste keeps working because the surrounding driver already holds the
iteration state the helper used to receive as arguments, and the end-to-end
fixture corpus keeps passing because outputs do not change. The revision
modeled here is the endpoint of several such individually defensible
consolidations: the pipeline drivers replay their sub-steps inline, the
helpers that lost their last caller are gone, and the helpers several
binaries still call survive with their original bodies.

## Overall design

Five driver functions across the four production files take part, covering
every stage family of the toolchain:

1. `kpatch_correlate_elfs` (create-diff-object.c) — the correlation driver.
   Its three stages (sections, symbols, static-local reconciliation) are
   replaced by inline re-implementations, and the static-local stage embeds
   the twin-search step one level further down, so the driver performs what
   used to be three calls deep.
2. `create-diff-object.c:main` — the comparison driver plus output finishing.
   The whole comparison chain (compare elements -> compare sections ->
   compare a correlated section -> compare a rela or non-rela section ->
   compare two individual relas) is inlined into the driver's section walk,
   and the epilogue inlines the rela-section data rebuild it used to call per
   rela section.
3. `lookup_symbol` (lookup.c) — the parent-object symbol binding routine. Its
   three scope stages (object locals, globals/weaks, exported symbols) are
   replaced by inline blocks inside the one lookup function.
4. `create-klp-module.c:main` — the KLP format exporter. The .klp.rela/.klp.sym
   expansion stage (find-or-add a klp symbol, find-or-add a .klp.rela
   section), the .klp.arch merge stage, and both cleanup passes (arch
   sections, intermediate .kpatch sections) run inline.
5. `create-kpatch-module.c:main` — the patch-module packager. The
   .kpatch.dynrelas construction stage (allocate the section and its rela
   partner, then fill the dynrela table), the intermediate-section cleanup,
   and the output rela rebuild all run inline.

Every pasted body preserves observable behavior exactly, including allocation
order, error messages, log lines, and the linked data seen by the unit
fixtures.

## Location rationale

### `kpatch_correlate_elfs` (kpatch-build/create-diff-object.c)

Production role: runs the correlation half of create-diff-object; the twin
pairings it establishes are consumed by every later phase (comparison,
inclusion, section/sym index building, output writing).

Why this location and shape: correlation is the tool's hottest, most
arch-specific reasoning path, and the static-local stage is the only code in
the tree that knows how gcc numbers those mangled suffixes. Recent fixes kept
touching this function, so the developer worked here directly. The three
stage helpers it called (`kpatch_correlate_sections`,
`kpatch_correlate_symbols`, `kpatch_correlate_static_local_variables`) had no
other caller and were absorbed entirely — including the static-local
stage's own helper, the twin search, whose parent-section fallback logic
becomes a nested block under the rela walk.

The section-pair correlation step, by contrast, is still needed as a helper
by other paths in this file — so the driver embeds a renamed clone of the
helper body (locals `cs_orig`/`cs_patched` bound to the enclosing loop's
section pair) instead of deleting the helper. The clones exist because the
helper reassigns its parameters for rela sections (it walks to the base
section), and the enclosing `list_for_each_entry` iterators must not be
repointed. The bundled-symbol path keeps a real call into the surviving
helper, since that is the one correlation case the driver still delegates.
Variable names, the bundle flags, the group-section byte comparison, and the
warnings for uncorrelated static locals keep their original meaning.

### `create-diff-object.c:main` (comparison chain + output finishing)

Production role: turns correlation into a SAME/CHANGED/NEW verdict per
section and symbol (this gate decides what ships in a patch) and finalizes the
output object: rela headers get sh_link/sh_info, rela data buffers get
regenerated from the relas lists, then strings, symbols, and sections get
tabulated before writing.

Why this location and shape: the chain that decides "what counts as a
change" is one deep call ladder with dense arch-specific rules (the toc-based
rela comparison, altinstr_aux tolerance, macro/line-number-only reverts,
subsection moves). All of its helpers were consulted from this phase only, so
the ladder got folded flat into the driver's section and symbol walks. The
fold keeps the original checks and short-circuits (mcount/sframe/
patchable-function-entries sections are declared SAME and rebuilt later; a
header mismatch is fatal; a rela whose only difference is the .altinstr_aux
addend is tolerated). The rela comparison helper returned a boolean from four
different exit points; inside the loop it becomes a result flag plus a
checkpoint label, because the enclosing loop must keep walking rela lists
instead of returning from the function — the form an inline paste naturally
takes. The section header log at the end of the stage is reached through a
local label standing in for the helper's return, and both rela and non-rela
branches assign the section status before it. A genuine call into the
macro-change checker and into the parent-profiling checker remains, since
those two helpers serve other callers as well.

The epilogue inlines the rela-data rebuild the driver previously invoked for
every rela section: the buffer count/copy loop is short, and the stage it
belongs to already walked all output sections when the section headers were
updated, so the rebuild sits in the same loop body over an alias of the loop
variable. The helper itself stays because the KLP module builder still calls
it.

### `lookup_symbol` (kpatch-build/lookup.c)

Production role: resolves a symbol name against the parent object (vmlinux):
first from the object's own local symbols, then its globals/weaks, then the
exported-symbol table. The scope order is a binding precedence, and the
result struct (objname, addr, size, sympos, exported flags) drives static
local correlation and required-symbol checks downstream.

Why this location and shape: the three scope helpers each needed the table,
the key, and the mutable result, plus their own early-exit conventions, and
recent fixes (disambiguation messages, sympos semantics for locals versus
globals) kept touching two of them at once; moving the stages into the one caller
avoided reworking the shared lookup interface every time a stage needed
another piece of driver state. All three stage bodies belong to this function
now, and the two
single-use helpers (`lookup_local_symbol`, `lookup_global_symbol`) no longer
exist.

The exported stage was folded at the function's top-level tail, so its early
returns simply become the driver's returns. Its loop counters got fresh (i,
ei) names to keep the stages readable side by side, and a local name copy of
the symbol name feeds the string comparisons. The exported helper itself
survives: the is-exported probe and the external symbol scan still call it
through its own name-based entry point.

### `create-klp-module.c:main` (klp synthesis, arch merge, cleanup)

Production role: converts the unified `.kpatch` module into the
kernel-linkable `.klp` format — creating per-object `.klp.rela.` sections and
`.klp.sym.` symbols — optionally merges `.parainstructions`/`.altinstructions`
content into `.klp.arch.` sections (the module loader would otherwise apply
them too early), and drops the tool-internal intermediate sections
(`.kpatch.symbols`, `.kpatch.relocations`, `.kpatch.arch` and their rela
sections) from the shipped module.

Why this location and shape: the expansion stage builds its own structures
while it walks the unified relocation table — the klp symbol and its
`.klp.rela.` section are created on demand and are looked up among sections
and symbols created in earlier iterations — so its two find-or-add helpers
would each need to return several side-channel values; keeping them separate
while their state is in the driver's locals pushed the developer to fold
them in. The arch merge copies entries while growing a merged buffer
(parainstructions alignment quirk included) and was folded for the same
reason. The cleanup passes then replay the generic remove-and-free helper of
kpatch-elf.c twice — once over an arch-section name array, once over the
intermediate name array — because in this driver a pass is just a name list
walk. The helper keeps its other real callers, so the tree now holds the
removal walk twice in one function and once in the shared file.

### `create-kpatch-module.c:main` (dynrelas + rela finalize)

Production role: packs the patch module: builds the `.kpatch.dynrelas` table
(used by the kernel module loader for late symbol binding) together with its
rela section, removes the intermediate `.kpatch.{symbols,relocations}`
sections, and writes the output ELF with consistent indexes and rela data.

Why this location and shape: the dynrela fill immediately follows the
allocation and reads its keys (src/dst objects, names, objname offsets) from
tables still addressed through the driver's pointers; combining allocation
and fill in the driver avoided passing the freshly growing data structures
around. The allocation path embeds the shared single-section creation body
(the element size is dynrela-specific while the helper is generic, so the
copy adds a local size seam between the count and the table element size)
followed by the tail half of the section-pair creation as a separate stitch
so that dynrela entries can be appended to the produced buffer. At cleanup,
the general removal walk is replayed over the intermediate-section names as
in the KLP driver. At output time, the rela section data regeneration is a
short buffer reindex, so it is inlined in the output loop.

### `kpatch-build/kpatch-elf.c`

Unchanged on purpose: it is the definition side of the shared helpers that
other binaries still call (section pair creation, symbol strtab/symtab
construction, section removal, rela data rebuild). Amending those helpers for
one caller's new needs is exactly the coupling the modeled developer avoided;
by contrast, leaving them alone keeps a coherent reference implementation of
each shared step, so any later decision to eliminate the drivers'
re-implementations has a place to return to.

## Cross-file coherence of the revision

The same consolidation style, applied for the same reasons, appears in every
stage family: correlation (twin pairing), comparison (change gate), relocation
and symbol synthesis (per-object loop emitted by two different formats),
cleanup (name-array driven removal in two builders), lookup (bonding stages
by scope), and output finishing (buffer rebuild reached from two binaries).
The resulting revision is one coherent piece of work: "the tools' pipeline
drivers now execute their sub-steps inline instead of delegating to their
stage helpers", readable as a single maintainer's campaign across the four
files, with no unrelated edits and no behavior change (the unit corpus
exercises every stage end to end).
