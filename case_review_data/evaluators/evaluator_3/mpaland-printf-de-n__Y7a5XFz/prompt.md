# Clean out the remains of the truncation-safety experiment

While wrapping up the follow-up work to the issue #45 `snprintf` reporting fix I
finally went through the tree, and we are still carrying a half-built idea from
a while back: a "dry-run" scheme that would compute how long a formatted result
will be without actually storing any characters, so that buffers could be sized
in advance and truncation could be caught before it happens. Whoever sketched it
got pulled away, and none of it was ever connected to the library's public
functions. The library works and all the tests pass, but the sketch is threaded
through the formatting engine and keeps misleading anyone who opens the file:
helpers written to look like the rest of the code that nothing calls,
bookkeeping that gets written and never read, a disabled debug peek into the
parameter parser, comments promising accounting that never materialized, and a
public header that still advertises a function that doesn't exist anywhere in
the tree.

Parts of it genuinely look alive at a glance. Some of it sits right next to the
real output plumbing, some of it mirrors the shape of the shipped conversion
code, and at least one piece only looks busy because another part of the same
dead sketch points at it — so a careless sweep both misses things and risks
taking live machinery with it. A build with our usual flags will point you at
some of it, but far from all of it, so don't rely on the compiler to find the
boundary of the experiment for you.

Please remove all of it, so the library is back to the state it should have been
in after issue #45: only the working implementation with nothing half-wired, no
scaffolding, no stale narration comments, and nothing left behind for the next
person to trip over.

## Constraints

- Every public function must keep its documented behavior: `printf`,
  `sprintf`, `snprintf`/`vsnprintf`, `vprintf`, and the output-function
  variant behave exactly as they do today, outputs stay byte for byte
  identical for everything the test suite covers, the `_putchar` integration
  is untouched, and the truncation-reporting return semantics settled for
  issue #45 stay as they are.
- The full existing test suite passes, without modifying it.
- Don't remove or alter anything the library genuinely uses. The stranded
  experiment leaning on live machinery does not make that machinery part of
  the experiment.
- Do not add new API surface while cleaning up.

Investigate across the whole implementation and its public header rather than
stopping at the first few spots, and make sure nothing orphaned survives when
you're done.
