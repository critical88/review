# Uniform container facade for the ut* family -- injection design record

Task: `troydhanson-uthash-is-e` (uthash @ a49bed0, assigned smell: interface_segregation,
explicit prompt mode). This file records why the injected change looks the way it does:
the maintenance motivation, the development evolution it models, the overall design,
and a per-cluster rationale for every materially changed location.

## Maintenance motivation

The ut* family ships six sibling container kinds, each a self-contained header with a
macro API (uthash, utarray, utlist, utstack, utstring, utringbuffer). Downstream users
keep asking for one question to be answerable generically: "I have a container -- any
container -- how do I empty it, walk it, add to it?" Today answering that requires
committing to a specific header and its specific macros, so generic code (serialized
walker in a networking daemon, a table of heterogeneous containers in an interpreter,
unit-test harnesses) gets written six times or driven by an if/else on the kind.

The realistic response a maintainer of this family would produce is a small uniform
facade: a generic handle type, an operations record that names the family's verbs,
per-kind adapter publications, and a handful of `ut_container_*` helpers so the generic
side holds one interface. That is the classic C idiom (kernel `file_operations`,
glib interface vtables, proto dispatch tables), and it is exactly the surface on
which interface-segregation pressure appears in C: there are no classes or interfaces
in the language, so "the interface" is literally the one record the handle points at,
and whatever it contains is what every kind must publish.

## Modeled development evolution

The change is shaped like normal growth of this repository, not like one big-bang edit:

1. A new self-contained header `src/utcontainer.h` appears, defining the handle, the
   operations record (members grouped by the container vocabulary they come from), and
   the dispatch helpers. This mirrors how utstack.h was forked from utlist.h and how
   utringbuffer.h grew out of utarray.h style: a fresh header, an `UTxxx_UNUSED`
   attribute macro, a version macro, no new object files.
2. Each existing container header gains an adapter block at its tail -- the established
   spot for additive sections in these headers (see the substring-search appendix in
   utstring.h, the `utarray_str_cpy`/`ut_str_icd` tail section in utarray.h). Each block
   is self-contained: static functions with narrowed signatures, one `static const`
   publication, a wrap constructor, all under the header's own unused-attribute macro
   so a translation unit that never wraps that kind pays nothing.
3. A consumer program `tests/test103.c` joins the suite the way every test in this
   repository does: one file under `tests/`, registered in `tests/Makefile`, with its
   recorded output kept as `tests/test103.ans` (the suite's own answer-file convention,
   same as test1..test25-style programs).

The point of modeling evolution this way: each adopter header was "migrated onto the
facade" one at a time, the way a real cross-header refactor actually proceeds, and each
migration had to decide locally what "adopt the facade" means for that kind.

## Overall design

- One operations record `UT_ops` in `src/utcontainer.h` with 16 function-pointer members
  -- the union of every family vocabulary: lifecycle (`clear`/`free`/`count`), keyed
  store (`add_key`/`find_key`/`del_key`), append/push (`push`/`top`), consuming the
  newest element (`pop`), indexed access (`at`), fixed capacity (`capacity`), traversal
  (`first`/`next`), ordering (`sort`), reversal (`reverse`), and formatted text growth
  (`vprintf`).
- The handle `UT_container` holds the wrapped object pointer plus one pointer to the
  kind's `UT_ops` publication. The 16 `ut_container_*` helpers dispatch straight
  through it. That single dispatch slot is what makes the ABI trivial and uniform --
  every kind, every caller, one interface.
- Because no kind serves the whole union, every adapter publishes the members it
  cannot honor as explicit refusals: `static` trampolines named
  `<kind>_ops_<member>_unsupported` that ignore their parameters and answer with the
  refusal value documented on the member (`NULL` for pointer results, `0` for counts and
  capacities, -1 for int results, no effect for void results). Keeping the record
  totally filled is presented as the price of the single dispatch slot.
- Where the wrapped kind stores caller values indirectly (hash, list, stack), the
  adapter keeps a small facade-owned box record behind `c->state`, because the facade
  promises that wrapping needs nothing from the caller's element types. Where the
  wrapped object already stores directly usable bytes or pointers (array, ring, text),
  the adapter wraps the caller's object itself and takes ownership in the constructor.

## Per-cluster rationale

### `src/utcontainer.h` (new facade header, +188 lines)

The header owns the whole contract and nothing else: `UTCONTAINER_UNUSED` (mirroring
`UTARRAY_UNUSED`, defined per-header in this family so each stays standalone), the
handle, the record, and the 16 helpers, each documented with its refusal convention on
the member it dispatches to. `ut_container_printf` is the only variadic member -- it
forwards through `vprintf` with a `va_list`, the standard shape for printf-like
vtable slots in C. The record's comment groups members by vocabulary ("lifecycle",
"keyed store", ...) -- the union was a deliberate design statement: one operation set
covering every family vocabulary, so additions never need a second record. The
downstream cost of that statement (every publication must fill all of it) is spread
across the adopter headers below.

### `src/utarray.h` adapter (tail block, +106)

`UT_array` already stores contiguous slots chosen by a `UT_icd`; the facade element is
the pointer value (`ut_ptr_icd` shape), so `at`/`first`/`next`/`top` dereference the
slot and `push` appends the caller's pointer. `sort` is a hand-rolled insertion sort
over pointer slots (the icd's `copy`/`dtor` never fire for pointer values, and it
avoids `qsort` on possibly-NULL storage when the array is empty). The constructor
`ut_array_container(UT_array*)` borrows the caller's array and says so ("adapts it
tightly; the array must not be freed while in use") -- mirroring how utarray itself
treats arrays as caller-owned. The kind has no keyed store, no native capacity bound,
no reversal macro in the utarray vocabulary at this revision, and no per-byte text
growth, so those six members are published as refusals.

### `src/uthash.h` adapter (tail block, +180)

Keyed store is the one vocabulary only this kind owns, and the facade cannot ask the
caller to embed a `UT_hash_handle` in their own structs -- wrapping existing tables by
reference would have made the element type the facade's business. Instead the adapter
keeps its own box: `struct ut_hash_box` (the item list plus the active sort comparison)
and `struct ut_hash_item` (owned key copy, caller's value, back-pointer to the box, and
the `UT_hash_handle`), so the box is a genuine uthash table of facade-owned items and
the caller only ever hands over `char*` keys and `void*` values.

`add_key`/`find_key`/`del_key` walk the box's items through the table internals
(`HASH_JEN`, `HASH_TO_BKT`, bucket-chain scan with `memcmp`, `ELMT_FROM_HH`) and insert
with `HASH_ADD_KEYPTR_BYHASHVALUE`, rather than the nicer `HASH_ADD`/`HASH_FIND`
macros. That is deliberate: those macros expand `HASH_FUNCTION`/`HASH_KEYCMP`, which any
including translation unit may have overridden before including `uthash.h` (an
overriding unit's functions describe *its* tables). The facade's box is this header's
own table, so it stays pinned to uthash's canonical default algorithm and bytewise
compare instead of inheriting whatever the includer redefined. `clear`/`free` delete
every item and release its key copy (the box owns both). Traversal yields the item
struct in application order (`first` returns the head item, `next` follows `hh.next`),
which is also what the keyed consumer prints; `sort` routes `HASH_SRT` through a shim
that forwards to the caller's comparator on the stored values, with the active
comparator parked in the box for the duration. The eight remaining members come from
vocabularies a keyed store does not have and are published as refusals
(`push`/`pop`/`top`/`at`/`capacity`/`reverse`/`vprintf`).

### `src/utlist.h` adapter (tail block, +151)

A generic list of caller values needs double-link semantics (append at tail, pop the
same end), so the adapter box holds element records with `prev`/`next` managed by the
`DL_*` macros: `push` allocates and `DL_APPEND`s, `pop` takes the last one out, `top`
reads it without removing, `first`/`next` walk by value pointer (matching the
traversal contract the helpers document), `count` uses `DL_COUNT`, `clear`/`free`
delete and release elements. `sort` rides `DL_SORT` through the same comparator-shim
pattern as the hash adapter (both boxes park the active comparator because list and
hash sort callbacks hand the *element record* to the callback, while the facade
promises comparisons on caller values), and `reverse` is a straight `DL_REVERSE` -- a
real capability of this kind and of no other in the family. The remaining members
(keyed store, indexed access, fixed capacity, formatted text) are published as
refusals; the constructor `ut_list_container_new()` mallocs the box the way
`ut_array_container_new`-style helpers do in this family.

### `src/utstack.h` adapter (tail block, +132)

The stack vocabulary is the smallest in the family: `STACK_PUSH`/`STACK_POP` /
`STACK_COUNT` on a singly linked element heap, so `push`/`pop`/`top`/`count` fall out
directly. Traversal runs in age order -- `first` walks to the oldest element, `next`
returns the element *newer* than the one passed in -- keeping the facade's "walk every
element exactly once" promise without adding a second linkage: newest-first insertion
order plus next-is-newer covers the traversal without any list machinery. Upstream
utstack has no ordering or reversal macros at this revision, so `sort`/`reverse` are
published as refusals alongside keyed access, indexed access, capacity, and formatted
text -- eight refusals, the largest count in the family, which is what a stack earns
for having the narrowest real vocabulary.

### `src/utstring.h` adapter (tail block, +91)

Here the element is a byte and the adapter wraps the caller's `UT_string` directly (no
box): `push` appends one byte, `at`/`first`/`top` return addresses into the buffer,
`next` advances an address, `count` is the length, and `vprintf` forwards to
`utstring_printf_va` and returns the byte growth (the member-level doc promises -1 only
where the operation cannot apply, which is the published refusal for this member on
every other kind). Deliberate limitation kept here: `capacity` is a ring-buffer
vocabulary (space before overwrite), while a `UT_string` grows on demand, so it is
published as a refusal rather than conflated with dynamic length; `pop` is likewise
not part of any ut* text API; keyed access, ordering, and reversal are refusals.

### `src/utringbuffer.h` adapter (tail block, +94, -1)

`UT_ringbuffer` already handles pointer-sized elements and overwrite; the adapter wraps
the caller's ring by ownership (constructor `ut_ring_container_new(n)` creates it,
mirroring `utringbuffer_new`). `push` triggers overwrite of the oldest when the bound
is reached ("wrap a bounded uniform container that keeps the newest n elements"),
`capacity` -- the one kind this member is real for -- is the ring's `n`, `at` maps to
stored pointers, `first`/`next` walk ring positions in order of content insertion
index, `count` is `utringbuffer_len`. Overwrite means `pop` cannot behave like a stack
pop without throwing away ring semantics, so it is published as a refusal, along with
keyed access, ordering, and formatted text.

-1 deletion: the `#endif` line at the old tail moves (adapter block inserted before it).

### `tests/` (Makefile +2/-1, test103.c +203, test103.ans +64)

`tests/Makefile` registers `test103` in `PROGS`, matching the surrounding lines. The
program itself is a generic consumer: one self-contained file in the suite's style, it
includes the container headers and drives every kind through the helpers only - each
supported capability, and each refusal answer, once - printing one line per call. The
walk is factored into a `walk()` helper so each kind's traversal is demonstrated with
the same generic loop the facade exists to enable. A couple of results are staged into
local variables before printing where a line would otherwise have two
`ut_container_*` calls in one `printf` argument list (C leaves argument evaluation
order unspecified, and other programs in this suite already stage temporaries for that
reason). The keyed section prints the box's items through the same item structs the
traversal hands out, since a generic walker over hashed keys has no kind-specific
macros to lean on. `tests/test103.ans` holds the recorded output; the suite's perl
driver `tests/do_tests` picks up every `test*` program that has an `.ans` and diffs
the run against it, which is the exact-output convention this suite already uses.
Registering in `PROGS` makes `make -C tests` build the program like any other test.

## Deliberate structural variation, in short

Adapters vary in the shape a real one-kind-at-a-time migration produces: some wrap the
caller's object by ownership (array, string, ring), some keep a facade-owned box
because their elements must be allocated by the facade (hash, list, stack); three
comparators are routed with a shim because two containers pass element records to
comparators while the facade promises value comparison; traversal direction follows the
kind's real convention (list front-to-back, stack oldest-first, hash application
order, string and ring address/index order) rather than one forced direction. The
similarities -- the `static const UT_ops <kind>_ops` publication, refusal naming,
refusal-value conventions, constructors above the `#endif` -- are the shared scaffold;
the differences are the kind-specific judgement that each migration demanded.
