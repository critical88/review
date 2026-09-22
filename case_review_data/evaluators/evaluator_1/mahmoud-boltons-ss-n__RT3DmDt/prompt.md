# Please make our "no value" markers a one-place decision

I sat down last week to make what should have been a tiny cross-cutting
change, and it turned into a treasure hunt. I'd like us to fix the
underlying problem properly rather than keep paying interest on it.

The change I wanted: our internal "no value" markers — the little falsy
sentinel objects we use to tell "the caller didn't pass this argument" or
"nothing was captured here" or "this entry is gone" apart from real values —
print with a readable name, and I wanted to touch up how those names are
derived so they read better in tracebacks. In my head that's a one-line
change in one helper.

Except we don't *have* one helper. Module after module has grown its own
private little machinery for building these markers: I kept finding the same
story in the container types, in the caching helpers, in the URL handling,
in the iteration walker, in the socket wrapper, in the priority queue, in
the debug tracer, in the function-decoration helpers... every copy a bit
different. Some are bare marker classes instantiated once; some are factory
functions; one memoizes its markers in a registry; one validates names; one
attaches a human-readable description to each marker; one is assembled on
the fly with `type()`. No two are exactly alike, none of them share
anything, and none of them knows the others exist. So my name tweak is now
"edit every one of these private implementations and get each of them
right", plus a test pass for each.

I'm sure each copy made sense to whoever added it. But the next marker
decision will have exactly the same shape as mine, and I'd rather we stop
re-making it per module. What I'd like:

- Markers should come from one shared implementation that every module uses.
  Where that implementation lives is your call — I don't care which module
  owns it — as long as it's exactly one place, and every module that needs
  one of these markers gets it from there instead of building its own.
- Audit for it, please: assume a module has its own copy until you've
  checked. I found more than I expected, and not just in the places I
  happened to be reading first.
- Take the dead machinery out with the refactor — the local factories,
  marker classes, and registries that are no longer used shouldn't linger
  behind as private cruft for the next person to puzzle over.

Things that must not change while you do this:

- Every marker that consuming code compares with `is` today still exists at
  the same module-level names and still compares by identity. The
  missing-value markers, the "no default for this argument" marker, the
  lazy-deletion tombstones, the walk-termination markers, and the
  cache-call boundary marker are all load-bearing.
- Markers stay falsy and keep printing their readable names. A few of those
  printed names show up in docstring examples that run as tests, and in
  trace output — if a marker degenerates to
  `<object object at 0x...>`, that's a behavior regression, not a cosmetic
  nit.
- If any of these markers today survives pickling, copying, or deep
  copying for its callers, it still has to afterwards.
- Public behavior of every module stays exactly as it is, and the full test
  suite passes.

When you're done, the next "how do markers work" question should have a
one-place answer.
