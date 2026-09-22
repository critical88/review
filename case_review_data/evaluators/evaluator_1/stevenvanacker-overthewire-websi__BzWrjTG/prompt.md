# Clean up the leftover retired-feature state in the wargame level sources

## The problem

Going through the OverTheWire wargame sources, I kept finding leftover
definitions inside the standalone C level sources (the per-level challenges
under the `vortex`, `semtex`, and `blacksun` level series that each define a
`main()` and build to a small executable). They look like things that got left
behind when an older command/monitoring surface, an ops hook, a staged auditor,
an in-band tracer, or an on-wire protocol was wound down and torn out.

They come in a few shapes:

- `static` helper functions that nothing calls anymore -- they still read like
  plausible parts of the level (they use the level's own types and markers),
  but no live code invokes them.
- A `static const` table that maps old operations to handler functions, where
  the table is no longer consulted but its initializer takes the addresses of
  the handlers, so the handlers survive only because of that table. The table
  looks wired in while the whole thing is dead.
- File-scope `static` variables that the main path keeps writing but that
  nothing ever reads back -- "last seen X" / snapshot / ring values that look
  like telemetry for a reporter that never shipped. The writes are a dead side
  effect. As an example, a `static` that the main loop writes on every iteration
  while no other code reads it is dead state, and the loop's write exists only
  to feed it.

I want these cleaned up.

## What to do

Find the leftover retired-feature definitions across the level command sources
and remove them cleanly. Concretely:

- Delete each dead definition outright.
- When you remove a dispatch-table remnant, remove the handler functions whose
  addresses it took at the same time, so nothing is left half-referencing the
  removed state.
- For a write-only module variable, remove both the `static` declaration and
  the live-path statement that writes it, so the source still compiles with no
  dangling reference.
- Leave the live paths and everything they do untouched.

Don't try to keep what you remove, and don't refactor anything that isn't this
leftover state -- the point is a clean, compiling tree with the retired-feature
leftovers gone and nothing left pointing at them.

## Keep behavior byte-identical

Each level's observable stdout and exit status for a given input is the
contract; several levels are exercised across many input vectors and their
output is byte-pinned. Do not change any level's observable output -- in
particular be careful with any leftover write sitting inside a level's main
loop, since the loop's output is what is pinned. To reproduce a level's
behavior while you work, build it the way the tree does, for example:

```
gcc -w -fcommon -no-pie -fno-pie -ftrivial-auto-var-init=zero -I<repo root> wargames/<series>/<level>.c -o <level>
```

drive it with the same inputs before and after your change, and confirm the
output and exit code are unchanged. The deliverable is the cleaned-up sources
with the leftovers gone and every level's behavior preserved.
