# Refactor the uniform container facade's operations interface

## Context

This repository ships the ut* header-only container family (hash tables, dynamic arrays, doubly
linked lists, stacks, dynamic strings, ring buffers). `src/utcontainer.h` recently added a "uniform
container facade" on top of them: a generic `UT_container` handle that holds a pointer to the wrapped
container plus one pointer to an operations record, `ut_container_*` helper entry points that
dispatch through that record, and an adapter block at the tail of each container header that
publishes a `static const` instance of the record for its kind, so that generic code can hold and
drive any ut* container through the same calls.

The operations record is the union of the whole family's vocabularies: lifecycle (clear/free/count),
keyed store (add/find/delete by key), append (push/top), consuming (pop), indexed access (at), fixed
capacity, element traversal (first/next), ordering (sort), reversal, and formatted text growth. It
was built that way so the handle and the helpers could stay identical for every kind.

## The problem

No container kind can honor that union, yet every adapter has to publish the entire record to give
the handle its one pointer. Each adapter therefore fills the members its kind does not have with
"unsupported" stubs that ignore their parameters and answer with a refusal (NULL, 0 or -1, or no
effect), so every kind -- and every generic caller reaching through the helpers -- ends up depending
on the full merged surface: capabilities that exist only for a ring buffer, or only for text
buffers, or only for keyed stores, are still part of what a list or a stack is made to publish and
dispatch through. A capability added for one kind ripples into every sibling publication, and "which
operations does this kind actually serve" can only be learned by calling and reading a refusal.

This is an interface-segregation problem: the facade replaces one size-fits-all dependence on the
merged record with per-kind dependencies the kinds cannot actually use, instead of making each kind
offer exactly what it serves.

## Desired outcome

Rework the facade so a container kind depends on, and offers, only the capability groups it
genuinely serves. Keep it a facade: the areas involved are the record and the helper-dispatch design
in `src/utcontainer.h` and the adapter publications in the container headers -- the exact shape of
the replacement is your design decision (focused per-capability records, per-capability pointers in
the handle, or another structuring you can defend).

Requirements:

1. The `ut_container_*` helper API keeps its names, parameter and return types, and calling
   convention. A kind that cannot serve an operation answers with exactly the refusal behavior the
   facade documents today (for example, a negative return from the formatted-growth helper and a
   NULL from indexed access on kinds that have neither) -- the refused-operation behavior is part of
   the published contract and consumers rely on it.
2. No adapter carries or reaches through operation members its kind cannot implement. The
   "unsupported" stub machinery and the merged record cease to exist in the shape that forced them;
   whatever dispatch structure remains is the one your design genuinely needs, and no dead
   functions, records or intermediate tables are left behind.
3. Generic code that includes only the facade header and the container headers keeps compiling and
   behaving as before: the existing wrap constructors still return a usable handle for every kind,
   and the full test suite keeps passing, including the facade consumer `tests/test103` whose
   recorded output `tests/test103.ans` must be produced byte-for-byte unchanged.
4. The pre-existing macro APIs and the public object layouts of every container header are frozen,
   as are the `include/` distribution copies; the headers stay self-contained and macro-only (no new
   translation units, no new externally linked symbols).

Whenever two capabilities are always served together by every kind that serves them at all, keeping
them together is fine; the point of this change is the forced dependence on operations a kind
cannot provide, not a hunt for the smallest possible record.
