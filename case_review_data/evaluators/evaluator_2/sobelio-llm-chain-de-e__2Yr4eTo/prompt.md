# Finish the compile-out migration of the retired pipeline implementations

## Background

This Rust workspace is a library for building applications on top of large language models. The cleanup targets the three pure-Rust crates of the workspace: the core library, the OpenAI (ChatGPT) integration, and the in-memory mock executor. The llama.cpp binding crates are out of scope.

A while back this codebase went through a modernization pass — the model APIs we reworked, a set of model variants were deprecated, and the prompt/tool/streaming machinery was rebuilt. During that rollout, the old behavior was kept compilable behind ordinary compile-time switches so the release train could keep moving. The migration was never finished: those switches are plain `const bool` items (not Cargo features, not `cfg` attributes), and their values are fixed in the source as it stands today.

## What we observed

Several behaviors in these crates are selected by `if` statements whose conditions evaluate to a fixed boolean at compile time. The condition is either one of the switch constants directly, a negation of one, a `&&`/`||` combination, or a derived constant built from other switches; a few conditions mix a dead constant with a real runtime expression through short-circuiting, in which case the non-constant part is still reachable. The side that always executes is what the library actually does today. The other side is unreachable code that still gets compiled, read, and reviewed.

The switches are organized in two ways: a dedicated switch module in the core crate holds most of them (including one derived from a combination of the others), while a few were declared locally next to their consumers in the integration crate. Example symptoms, illustrative and not exhaustive:

- The conversation chain retains a complete alternate context-trimming strategy behind a permanently disabled switch; the path that runs today trims history by token budget.
- The ChatGPT integration still contains unreachable formatting variants for how model names are rendered and how chat roles are mapped.

The same pattern exists in other parts of the core library — the tool, token, parsing, streaming, and prompt-assembly paths — and in the mock executor. Audit the affected subsystems; do not stop at the first sites you find.

## What to do

1. Locate every `if` in the three pure-Rust crates whose branch selection is decided by a compile-time constant: direct constant references, negations, `&&`/`||` combinations, constant-to-constant chains, and short-circuit combinations where only part of the condition is a dead constant. Resolve each constant to its actual value in the source as it stands; do not assume.
2. For each site, determine which side of the branch always executes today. Remove the permanently unreachable side, keeping the running behavior exactly as it is now.
3. Remove the leftover machinery: switch constants that no longer feed any decision, the switch module once nothing references it, and any private helper function whose only callers were inside the removed code. Avoid leaving items that the compiler will report as never used afterwards.
4. Work site by site from the resolved constant values — a single global edit (for example, flipping every switch value) is not a resolution: it would change which code runs and which code is dead, not remove the dead code.

## What must remain unchanged

The change must be invisible at runtime — a reader diffing your patch should see dead code disappearing, nothing else switching on.

- Existing tests must pass unmodified. Do not switch any behavior over to the currently-unreachable implementation.
- The rendered strings and formats the library produces stay exactly as they are today: model-name strings, chat role mapping, chat token accounting, embedding extraction results, the machine-readable tool description format, tool-invocation extraction from model output, prompt combination semantics, prompt text splitting, streamed reply assembly, and conversation history trimming.
- The public API surface stays intact: public types, traits, and functions keep their names, visibility, and signatures — including the deprecated model variants, which are deprecated but still public API, not dead code. Internal signature simplifications are acceptable only where behavior and visibility are preserved.
- Runtime-configurable behavior is not dead code. In particular, the environment-variable override for the conversation token budget is intentional, runtime-driven behavior and must keep working. Do not confuse "selectable at run time" with "permanently disabled at compile time" — only the latter is in scope for removal.
- The normal `cargo build` and `cargo test` commands for the workspace's pure-Rust crates must succeed when you are done.
