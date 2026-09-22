# Injection design record — retired pipeline switches in `llm-chain`

## Maintenance motivation

This workspace went through a sizable modernization: the ChatGPT model surface was reworked and a family of model variants was deprecated in place, and the prompt, tool, token, and streaming machinery was rebuilt around the new abstractions. Migrations of that size are usually rolled out in two moves: first the new implementation ships while the old one is kept compilable behind a compile-time switch, and later the old code is deleted once the new path is proven. The second move is the one that historically never happens — the rollout finishes, other work piles up, and the disabled paths stay in the tree for years, still compiling, still being reviewed, still misleading readers.

The modeled evolution here is exactly that intermediate state left to rot: a set of retired pipeline stages kept behind permanently-set compile-time switches after the modernization completed. Switch-driven retention is a recognizable way real Rust projects stage removals: the stages are `const bool` items (cheap to read, no Cargo feature surface churn), the disabled paths stay in the same functions as the live ones, and the constants themselves accumulate doc comments describing stages that no longer exist.

## Overall design

The switch definitions are organized the way a real cleanup-in-waiting would be:

- **A dedicated per-crate switch module in the core crate** (`legacy.rs`, declared from `lib.rs`) holding the retired-stage switches as documented `pub(crate)` boolean constants, each named after the stage it gates. One "pipeline" switch is *derived* — a conjunction of two other stage switches — which is how composite feature toggles usually accrete.
- **Module-local switches in the sibling crates**, declared next to their consumers in the OpenAI and mock crates, the way teams wire local toggles without reaching for a shared module.
- **Guard shapes that reflect real condition-writing styles**: a plain constant guard, a negated guard, a derived-chain guard, a constant-true guard (disabling the *other* side), and a short-circuit conjunction where a dead constant is mixed with a genuine runtime expression. The guarded sides include if/else bodies, an early-return block, and a tuple selection in expression position.
- **Private helper functions that serve only the retired sides**, which is what naturally happens when the disabled path needs its own formatting or arithmetic: the helper is written for it and nothing live calls it.

Two small unrelated maintenance edits ride along, of the kind that normally share a housekeeping commit: an operator-facing environment-variable override for the conversation trim budget, and two doc-comment typo corrections in the parsing module.

## Per-cluster rationale

### Core crate: the switch module (`crates/llm-chain/src/legacy.rs`, `crates/llm-chain/src/lib.rs`)

A new module holds six retired-stage switches (context trimming, tool descriptions, the token splitter, stream assembly, YAML fence parsing, prompt combining) plus one derived pipeline switch combining two of them. This is the standard shape of a "flags of things we are about to delete" module: one item per retired stage, doc comments narrating why each stage is kept, and a composite toggle for callers that care about the pipeline as a whole rather than individual stages. The `pub(crate)` visibility is what a crate-local migration actually uses — no reason to publish removal-pending knobs. Declaring the module from `lib.rs` is the minimum registration needed for the rest of the crate to reference it.

### Conversation chain context trimming (`crates/llm-chain/src/chains/conversation.rs`)

`Chain::send_message_raw` trims conversation state before each step so prompts fit the model budget. The live implementation is token-budget-driven (`trim_context`). The injected cluster adds the *previous* trimming strategy — a message-count-style trim with its own target computation helper — behind the stage switch, as an if/else on the trimming call site. The cluster also gains the trim-budget override plumbing mentioned above, which fits the surrounding code's operator-knob style. This site was selected because context management is the core lifecycle responsibility of the conversation chain, and trimming is a behavior with a naturally dead predecessor: budget-based trimming replaced count-based trimming across LLM framework generations.

### Tool description rendering (`crates/llm-chain/src/tools/collection.rs`)

`ToolCollection::describe` renders the registered tools into the piece templates use to tell models what they may invoke; the live output is a machine-readable (serde-serialized) description list. The modeled predecessor is a hand-formatted plain-text listing with a small renderer helper for one tool's description fields. Tool-use formats migrating from prose descriptions to structured (YAML) descriptions is the actual history of this space, so a retired renderer behind a switch is a natural fit. This site also gives the tools subsystem its own manifestation rather than leaning on the prompt path alone.

### Token splitting (`crates/llm-chain/src/tokens.rs`)

The `Tokenizer` trait's default `split_text` splits long texts into overlapping chunks. The injected predecessor is a different bounds computation with its own stride arithmetic helper, wired as an early-return block ahead of the live splitting loop. Early-return blocks are how "old algorithm first, new algorithm below" get staged during a cutover. The trait default was selected because it is the shared contract implementors inherit, making the retired path visible to every consumer of the trait rather than one implementation.

### Tool-call extraction (`crates/llm-chain/src/parsing.rs`)

`find_yaml` extracts structured tool invocations from model output; the live code scans the generic fenced-block form. The modeled predecessor extracts from a stricter fenced plain-text form specifically, as a scan stage ahead of the live one. Parsing markdown-delimited LLM output is exactly the kind of code that gets versioned during prompt-format changes, and the two doc-comment corrections (grammar in an error-description comment, a stale sentence in the module docs) belong to the same copyediting pass.

### Streamed reply assembly (`crates/llm-chain/src/output/stream.rs`)

`OutputStream::into_data` assembles received stream segments into the final data value. The injected predecessor is a self-contained reassembly loop reading from the same segment receiver as an early block before the live single-pass assembly. Streaming implementations churn whenever the interim buffering strategy changes; the early-block shape models a replaced segment-consumption pass left in place. This receiver-taking block is also why the function's receiver is declared `mut` in this state.

### Prompt combination (`crates/llm-chain/src/prompt/model.rs`)

`Data::combine` defines the merge semantics for the two prompt shapes (chat collections and plain text). The live semantics are the original match on the pair of variants. The modeled predecessor is a simpler chat-append merge: fold everything into a chat collection and append. Combination semantics is one of the most consequential prompt-pipeline behaviors, so the derived pipeline switch (itself a conjunction of two stage switches, negated at the call site) is the natural gate for it — this is the cluster where a composite toggle earns its keep, and the negated derived-chain shape distinguishes it from the plain stage-switch clusters.

### Model-name rendering (`crates/llm-chain-openai/src/chatgpt/model.rs`)

Model-name strings in this integration are in active churn — the enum already carries `#[deprecated]` variants left over from the API transitions. The modeled cluster adds a rendering-time aliasing switch: while some deployments referred to model variants with a provider-prefixed spelling, an alias formatter existed for compatibility; with that era over, the switch guarding it is permanently off. A local constant fits: the alias concern is this file's alone, and the deprecated-variant churn already localized naming maintenance here.

### Role mapping and message accounting (`crates/llm-chain-openai/src/chatgpt/prompt.rs`, `.../chatgpt/executor.rs`)

Two small formatting behaviors in the request path: (a) `convert_openai_role` maps the abstract role onto the wire role; the modeled predecessor collapses two roles that an earlier protocol version did not distinguish; (b) `num_tokens_from_messages` estimates the overhead per message; the modeled predecessor is a flat accounting with no model-family sensitivity. Both are guarded by module-local switches declared beside them. These were picked to give ChatGPT request assembly more than one manifestation in different guard positions — an if/else in a mapping function versus a tuple selection inside an arithmetic expression's arms.

### Embeddings extraction (`crates/llm-chain-openai/src/embeddings.rs`)

`embed_query` unwraps the API response for the single requested vector. The live code takes the final embedding directly; the modeled predecessor collected all vectors and truncated to a fixed dimension — the kind of normalization code that stays on while an API's era of fixed-shape vectors phases out. The switch guarding this cluster is the one declared `true`, so it is the *live* extraction that would keep running and the normalization path that is disabled, giving this cluster the opposite dead-side orientation from the plain stage switches.

### Mock executor formatting (`crates/llm-chain-mock/src/executor.rs`)

The mock executor echoes prompts back for tests and examples. The modeled cluster is a formatting gate for short prompts with a different echo formatting, short-circuited via `&&` with a genuine runtime length expression. The short-circuit shape is the realistic way conditions like this get written — a length guard that only mattered under a format nobody uses anymore, composed with the constant that retired the format. It also makes the mock crate, which downstream consumers read as a usage example, carry the pattern rather than being the only integration untouched.

## Sites surveyed and not selected

The prompt templates, frame/step plumbing, parameters/options, schema, serialization, document stores, summarization, and agents modules were considered and skipped: they are data-passing or config code where a retired stage would have no live counterpart to sit beside, or stages with no behavioral decision point of their own. The OpenAI streamed-output forwarder delegates assembling to the core stream type, so it has no local site for a retired variant. The llama.cpp binding crates were out of scope as the non-Rust surface.
