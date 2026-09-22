# One environment policy for the sandbox, please

Last week's ticket looked as simple as they come: pin one more environment
variable for every command the playground runs inside a sandbox. I mostly
maintain the sandbox-orchestration side of the Rust Playground — the crate that
turns a user request into the actual `cargo`/tool processes that execute in a
sandbox (`compiler/base/orchestrator`) — so I expected a one-liner somewhere.
Instead I spent the ticket discovering where the environment of an executed
command actually comes from.

There isn't one answer. Over the past months, every environmental guarantee we
lean on was added by whichever ticket first needed it, as its own little helper
function that contributes a few name/value pairs, sitting in whatever file was
convenient at the time: output must render without ANSI escapes, the locale
inside the sandbox is always the same, a user build doesn't get to use every
core, the sandbox clock is UTC, sandbox identity is visible to the processes we
start, and panics carry a backtrace when the user asked for one. None of these
landed together, and none of them knows about the others. Then, every place
that lowers a request into a command merges, by hand, whichever fragments that
kind of command seems to need — and each kind does the merge its own way.

So my "one-liner" turned into an archaeology dig: find every fragment, work
out which request kinds depend on which pieces, extend every merge, and also
check the places on the worker side that write the environment themselves —
both where processes get started and where the worker sets up its own
environment before it starts listening — because those don't go through the
request path at all. I did that for one variable and I'm already not confident
the next person will even find all of them. When I asked to see "the
environment policy of a sandbox", the honest answer was: a stack of greps.

That's the state I want to end. The environment every sandboxed process
observes should be a policy with an owner: one place where the whole baseline
can be read top-to-bottom and changed in one edit, that the rest of the crate —
the request lowerings, the process starting, the worker startup — consults
instead of hand-composing fragments. Legitimate differences should survive as
policy in that owner (backtrace only when the request asks for it, miri's own
variables, the coordinator-sent settings taking precedence over whatever the
worker adds) rather than as knowledge spread across the call sites. Please
audit the whole orchestration crate for fragments of this baseline and the
hand-merges that consume them, not just the request kinds I happened to touch,
and also retire whatever generic merging glue was introduced along the way once
the new owner makes it unnecessary.

## Behavior that must be preserved exactly

- Run/build requests: `RUST_BACKTRACE=1` exactly when the request asks for
  backtrace detail, plus the output-rendering settings and the
  fairness settings (`CARGO_BUILD_JOBS=2`, `CARGO_NET_OFFLINE=true`).
- Format, clippy, macro-expansion, and miri requests keep the rendering
  settings (`CARGO_TERM_COLOR=never`, `LC_ALL=C.UTF-8`); miri additionally
  keeps its `MIRIFLAGS` and prebuilt `MIRI_SYSROOT` behavior, including the
  intent that no sysroot is rebuilt.
- Every process the worker starts still observes `PLAYGROUND_SANDBOX=1` and
  `TZ=UTC`, with coordinator-sent values still winning where both exist.
- The worker process still sets `TZ` (defaulting to UTC only when unset) and
  marks itself as the worker, once, before listening.
- The coordinator/worker wire protocol is unchanged, and every operation the
  playground supports behaves as before.

## Verifying your work

```console
$ TESTS_MAX_CONCURRENCY=3 TESTS_TIMEOUT_MS=30000 \
    cargo test --manifest-path compiler/base/orchestrator/Cargo.toml --locked \
    -- --skip miri --skip wasm --skip limited --skip disabled
$ cargo test --manifest-path compiler/base/asm-cleanup/Cargo.toml
```

Both suites must pass unchanged, and the crate must still build with
`cargo build --manifest-path compiler/base/orchestrator/Cargo.toml --locked`.
