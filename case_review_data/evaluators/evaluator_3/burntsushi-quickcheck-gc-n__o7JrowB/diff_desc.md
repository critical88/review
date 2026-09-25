# Injection design record — `burntsushi-quickcheck-gc-n`

## Maintenance motivation

The `quickcheck` crate is a small, stable library. Its public surface is
narrow, but the machinery behind it touches a lot of different concerns:
reading run configuration from the environment, preparing logging,
deciding what a passing, failing or discarded property invocation means,
capturing panics the property raises, formatting counterexample
arguments for the failure report, and shrinking randomly generated
values toward a minimal witness.

The design decision modeled here is the one small libraries make under
contribution pressure: the one type that carries the whole run — the
property-testing runner — is the most visible entry point, so each
incoming change that needs "the full picture" lands on it rather than on
the component that owns the concern. Done one pull request at a time,
each step is locally reasonable: the assignment is small, the behavior is
easy to keep, and the runner already imports everything the job needs.
The cumulative shape is what this case records: a single owner whose
method set now spans configuration reading, run preparation, outcome
construction, panic interception, failure formatting and value
shrinking, while the components those responsibilities came from are
left as thin shells or empty space.

This is normal evolution, not sabotage: no call is dead, no behavior is
changed, and every absorbed method is genuinely used by the run.

## Normal evolution modeled

The diff represents a sequence of routine maintenance commits:

- environment-variable reading that used to be module-level helpers next
  to the runner becomes named, documented methods of the runner
  ("expose the runner's defaults as part of its configuration story");
- the crate-root logging bootstrap moves in ("the runner should not
  depend on the crate root to prepare itself");
- the outcome type's own constructors turn into delegating shims ("the
  runner decides what a pass/fail is, so let it build the outcome");
- the free helpers that support the function-property machinery follows
  ("they only exist to serve the run, so they belong with the runner");
- the vector-shrinking state machine is served out of the runner so that
  two shrinking entry points ("just call the runner for this, too").

Each step takes the quickest reasonable path — the runner already sees
all of this state — and the result reads like an intentional
coordinator, which is exactly how such structures get defended in code
review.

## Overall design

The runner type ends up with twenty-four inherent methods spread over
two inherent impl blocks in two different modules: its original
configuration-and-execution core in the property-execution module, and a
second impl block inside the arbitrary-generation module holding the
shrinker logic. The methods group into clearly disjoint families:

- the original core: construction, builder-style setters, and the inner
  and outer run loops that consume the configured counts and generator;
- a configuration family reading `QUICKCHECK_TESTS`,
  `QUICKCHECK_MAX_TESTS`, `QUICKCHECK_GENERATOR_SIZE` and
  `QUICKCHECK_MIN_TESTS_PASSED` with the same fallback defaults as
  before;
- a run-preparation family that starts the logger (cfg-gated for the
  `use_logging` feature in the same shape as the old crate-root helper);
- an outcome-construction family (`result_from_bool`, `result_passed`,
  `result_failed`, `result_error`, `result_discard`, `result_must_fail`);
- a panic-interception helper (`safe_run`) and a formatting helper
  (`debug_reprs`) used by the function-property machinery;
- a vector-shrinker family (`new_vec_shrinker`, `vec_shrinker_next`,
  `vec_shrinker_next_element`) that constructs and drives the shrinker
  of another type through `&mut` borrows of that type's private state.

The outcome type keeps its public constructors but each now forwards to
the runner's outcome-construction family, and the shrinker type keeps
its `Iterator` impl as a one-line forward into the runner's shrinker
family. The two `Arbitrary::shrink` entry points for vector and
byte-buffer values call the runner's shrinker constructor. Nothing about
the public API, the exported macro surface, or the observable run
behavior changes.

## Per-location design and rationale

### `src/tester.rs` — configuration family absorbed into the runner

The four module-level helpers that read the `QUICKCHECK_*` environment
variables with their fallbacks are deleted and reappear as
`default_tests`, `default_max_tests`, `default_gen_size` and
`default_min_tests_passed` on the runner, and the constructor `new()`
threads through them instead. The `default_*` names follow the runner's
own vocabulary ("the defaults this run will use") so the absorbed logic
reads as if it had always been there; that is also what makes the
process look deliberate rather than accidental. Production role: run
configuration. This site was chosen first because it is the most natural
"the runner is the right home" argument a maintainer makes, and the
one-to-one helper-to-method mapping keeps the assembly reviewable.

### `src/tester.rs` — run preparation (logging) absorbed into the runner

The crate root's cfg-gated logging bootstrap is removed from
`src/lib.rs` and reappears as the runner method `init_logging()`, with
the same feature-gated pair of definitions, and the outer run loop calls
`Self::init_logging()` instead of reaching from the runner module back
into the crate root. Folding the two cfg-gated shapes into one named
runner method keeps the feature behavior identical while making the
runner responsible for its own preparation. Production role: run
preparation for logging. This step removes a crate-root function whose
sole caller was the runner, which is the kind of cleanup a refactor
narrative sells itself on.

### `src/tester.rs` — outcome construction absorbed into the runner

The outcome type's constructors (`passed`, `failed`, `error`, `discard`,
`from_bool`, `must_fail`) become thin delegates. The actual knowledge of
what a pass/fail/discard outcome is — the status variant, the message
slot, the panic capture that decides a must-fail test — moves into the
runner's `result_*` family (`result_from_bool` building the primitive
from a bool and the other family members layering discard, error and
panic-capture on top). Production role: verdict semantics of a property
invocation. This is the center of the ownership shift this case cares
about: the type that carries a verdict no longer constructs it, and the
construction rules now live with the run coordinator. Keeping the
original constructor signatures as delegates preserves the public API and
external callers untouched, mirroring how compatibility is kept in
place while internals shift.

### `src/tester.rs` — panic interception and failure formatting absorbed

The free helpers `safe` (catches a panic from the property and extracts
its payload) and `debug_reprs` (formats counterexample arguments for the
failure report) are deleted and reappear as the runner methods `safe_run`
and `debug_reprs`. The `testable_fn!` machinery that expands blanket
`Testable` impls for functions of each arity is updated to call the
runner methods. Production role: failure reporting. Absorbing these two
made sense to model because they are stateless: the runner does not need
any additional state to serve them, only the practice of routing every
part of the run through itself.

### `src/arbitrary.rs` — the vector-shrinker state machine, re-homed behind the runner

The strongest step: the shrinker type's inherent impl (its constructor
plus its element-iteration helper) is deleted, and an additional inherent
impl block of the runner is placed in `arbitrary.rs` with
`new_vec_shrinker`, `vec_shrinker_next_element` and `vec_shrinker_next`,
the state machine bodies moved verbatim with `self` re-targeted to an
explicit `shrinker: &mut VecShrinker` parameter. The shrinker type keeps
its struct (its private `seed`/`size`/`offset`/`element_shrinker` state
survives) and its `Iterator` impl, but `next()` is now a one-line call
into the runner's family. Both `Arbitrary::shrink` entry points that use
that machine — the one for vectors and the one that reuses it for
C-string byte buffers — call the runner's constructor. Production role:
value shrinking during failure minimization.

Two deliberate design choices here. First, Rust permits inherent impls
only in the crate that defines the type, so the runner's shrinker
methods must sit in a file of the defining crate — placing the impl in
the generation module rather than the runner's own module records the
responsibility spread across two files with an unusual shape that
cannot be dismissed as a same-file find-and-replace. Second, this is the
one absorption that threads *mutable* state: the runner's methods now
drive another type's private state through `&mut`, so the shrinker's
behavior and the runner's method set become interlocked while sharing no
fields of the runner at all.

### `src/lib.rs` — crate-root retraction

The crate root keeps the `info!` macro compatibility pair but loses its
`env_logger_init` bootstrap pair, whose responsibility now lives on the
runner. Production role: library glue. This is the only edit at the
crate root and it exists because the absorbed logging site needs its
former home emptied.

## Deliberate structural variation

The absorbed logic was deliberately placed in three different home
shapes so that no single extraction pattern resolves the case: same
file and same module (environment configuration, outcomes, panic and
formatting helpers), different module and different file as a foreign
inherent impl (shrinker machinery), and the crate root (logging
bootstrap, emptied). Some absorbed families touch no state at all
(outcome variants, formatting), one family reads the environment, and
one drives foreign mutable state. The origin components were left in
different post-absorption states — one hollowed into delegating shims
(the outcome type), one reduced to a forwarding iterator (the shrinker
type), one simply deleted (the free helpers, the crate-root bootstrap) —
so understanding the current structure requires reading all three files,
not one localized region.
