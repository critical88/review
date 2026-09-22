# The reporting helpers read like they live in the wrong place

## What we noticed

Last cycle this workspace grew a small reporting layer: helpers that
summarize what a physical column family actually stores (payload bytes,
rows, null rows, backing slots or boundaries, and the family metadata some
families carry), plus helpers that render one line describing either the
input columns of an evaluation batch or the declared signature behind a bound
expression.

I finally sat down to review the whole addition, and the same thing stood out
everywhere I opened it. None of these helpers really do their own work: each
one takes a single structure by reference, walks it from top to bottom through
that structure's accessors, and reduces it to a report. The storage audits all
sit together in one place, far from the families they summarize. The batch
narration is off in its own module. The signature narration hangs at the
bottom of the binding code. Everywhere, one file owns the data and a different
file decides what the data means.

That arrangement will keep biting us the first time a family's storage changes
or the view or binder grows new state — the fix will be two-file surgery again,
and reviewers reading a family's impl will still not be able to see that
somebody else is interpreting that family elsewhere.

## What we want

Move the reading logic so that each summary becomes an operation of the type
it summarizes. After the cleanup, someone editing a storage family, the
erased input view, or the bound expression should find the matching summary
logic in that structure's own impl, and the places that only exist to host
the relocated logic should not keep a copy of it lying around. The genuinely
family-agnostic pieces of the vocabulary — the report record itself and the
width and bit-count helpers that no particular type owns — should stay in
one shared place rather than being stamped out per family.

This workspace is a teaching codebase and its shape is the point, so keep the
change to this cleanup: no new dependencies, no behavioral redesign, nothing
touched outside it.

## What must stay true

- Every summary that exists today is still obtainable afterwards, byte for
  byte: the same storage counts and family metadata for any given input, and
  the same narration strings for the same batch or bound expression. Things
  that parse these reports and logs depend on their exact shape, so values
  and rendered text may not drift. How the summaries are exposed is up to
  you, as long as workspace code can still reach them.
- The rest of the workspace — evaluation, builders, binding, existing tests —
  behaves exactly as before. Run the full suite with 
  `cargo test --workspace --locked`; it must pass.
