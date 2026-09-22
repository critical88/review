# Cleanup: retired experiments that are still compiled into `conrod_core`

While preparing `conrod_core` for the next release we went through the parts of
the update and event-handling pipeline that still carry code from old
experiments, and we keep tripping over the same pattern: a fixed compile-time
setting that was meant to let us switch between a new behavior and the old one,
where the experiment was eventually abandoned and the setting was hard-wired to
one side. The abandoned side is still there. It can never execute, but it is
still part of the code every reader has to understand, and the helpers that
exist only to serve it still have to be kept compiling and correct.

The affected work lives in the parts of the crate that drive a frame update:
the UI's handling of incoming events (in particular the reaction to window
resizes), the global input state that aggregates events between updates and its
immediate-vs-buffered forwarding decision, the depth-order graph that decides
whether to re-sort widgets when a `UiCell` is closed, and the scroll state that
decides whether to recalculate a scroll offset or reuse the previous one. A
small shared geometry helper picked up along the way is also only used by the
abandoned paths.

Please remove this leftover scaffolding from `conrod_core`. Concretely, that
means the disabled experiment paths themselves, and everything that exists only
to support them: branches that the fixed settings can never select, buffer
state and index bookkeeping that no live path reads or writes, enum variants
that are never constructed, and any helper function that loses its last caller
once the unreachable paths are gone. The compile-time settings that remain
hard-wired to one value, along with their surrounding ceremony, should stop
being load-bearing for code paths that can never run.

The supported behavior must not change. The public API of `conrod_core`
(`cargo doc -p conrod_core` output surface, item visibility, trait
implementations) must stay exactly as it is, the full test suite of the crate
must keep passing, and every code path that executes today must behave
identically after the cleanup. The result should read like the experiments had
never been kept around after they were abandoned: no half-deleted planners, no
erstwhile "config" that only ever has one value, and nothing left whose only
reason to exist is a path that cannot be taken.
