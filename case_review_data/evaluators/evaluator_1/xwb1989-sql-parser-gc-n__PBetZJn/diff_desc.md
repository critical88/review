# Injection design record — God Class on the query formatter

## Maintenance motivation and modeled evolution

The `sqlparser` package produces several *kinds* of query text from a parsed AST, and each kind historically had its own owner:

- a **bound query** — a template with bind locations already recorded, into which supplied bind variables are substituted to yield an executable statement (`ParsedQuery.GenerateQuery` with `EncodeValue` and `FetchBindVar`);
- a **redacted query** — the same statement with literal values scrubbed for safe display (`RedactSQLQuery`, which calls `Normalize` and renders with `String`);
- an **impossible / shadow query** — a deliberately unsatisfiable form (`where 1 != 1`) that vtgate and vttablet use to fetch field metadata without selecting rows (`FormatImpossibleQuery`).

All three ultimately write into the same `*bytes.Buffer` and, when a node tree is involved, go through `TrackedBuffer` — the type that renders AST trees to query text and that owns the formatting primitives `Myprintf`, `WriteArg`, and `WriteNode`.

The change modeled here is a plausible but overzealous centralization that a maintainer might undertake under the banner of "make the rendering buffer the single surface for every query text we produce." The reasoning is seductive: the buffer already holds the byte sink and already renders node trees, so it must "know about" every query form that is emitted. Over a short series of commits the maintainer folds the bound-query construction, then the redaction, then the shadow emission onto `TrackedBuffer`, leaving the original owners as one-line delegators. The result is a class that renders templates *and* resolves and encodes bind variables *and* scrubs literals *and* emits shadow queries, with the bind-variable namespace threaded through it as shared mutable state.

## Overall design

Three foreign query-emission responsibilities are relocated from their existing owners onto the formatter type, `TrackedBuffer`, and the prior owners are reduced to thin delegators whose public signatures are unchanged. Two new fields on `TrackedBuffer` thread the state the relocated concerns need (a bind-variable namespace, a redact prefix), installed through chained builder setters that make the coupling read as deliberate configuration rather than happenstance. The bodies of the relocated concerns are carried over verbatim with only the receiver/buffer plumbing adjusted, so the rendered query text, tuple/list expansion, custom encoders, and error messages are identical to the clean implementation.

## Per-cluster rationale

### `tracked_buffer.go` — the formatter becomes the accumulator

`TrackedBuffer` is the only type in the package that declares a formatting primitive, so it is the natural and only home for a formatter-centered god class. The struct gains two fields:

- `bindVars map[string]*querypb.BindVariable` — the bind-variable namespace the bound-query materialization resolves names against, threaded through the buffer rather than held by the parsed-query routine;
- `redactPrefix string` — the prefix the redaction path uses to name generated bind variables.

and two builder setters, `SetBindVars` and `SetRedactPrefix`, both returning the receiver so callers can chain configuration when constructing a buffer for bound-query or redaction work. These setters exist to *cement* the coupling: instead of passing the namespace and prefix as parameters to the relocated concerns, they are installed on the long-lived formatter object, so the formatter now carries configuration that only the foreign concerns use.

Three concern bodies are added directly on the type:

- **Bind-variable materialization** — `GenerateBoundQuery` walks the parsed query's precomputed bind locations and writes the final substituted statement into the buffer, resolving supplied variables through `ResolveBindVar` and encoding them through `EncodeBindValue`; `ResolveBindVar` performs the single/list parsing and validation that `FetchBindVar` previously owned; `EncodeBindValue` performs the scalar-and-TUPLE expansion that `EncodeValue` previously owned. These reference `querypb.BindVariable` and the `sqltypes` value helpers, which is why the two imports are added to this file. Production role: the formatter, not the parsed-query routine, now owns producing the executable bound statement.
- **Literal redaction** — `WriteRedacted` calls the package-level `Normalize` with the buffer's own namespace and redact prefix, then renders the statement (with its leading/trailing margin comments) through `WriteNode`. Production role: the formatter, not the redaction entry point, now owns scrubbing literals for display.
- **Impossible-query shadow emission** — `WriteImpossibleQuery` carries the select/union/default switch and the `1 != 1` clause. Production role: the formatter, not the impossible-query entry point, now owns emitting the zero-row shadow statement for field-metadata fetches.

This site and implementation form were chosen because `TrackedBuffer` is the structural hub the three concerns already shared (the byte sink and, for redaction/shadow, the node renderer); folding the bodies onto it is the smallest edit that produces the god class while it is also the one edit a "single rendering surface" refactor would naturally reach for.

### `parsed_query.go` — the bound-query union becomes a delegator

`GenerateQuery` now constructs a `TrackedBuffer` carrying the caller's bind variables in its threaded namespace and delegates to `buf.GenerateBoundQuery`. `EncodeValue` and `FetchBindVar` become one-line delegators that wrap a `TrackedBuffer` around the supplied buffer/namespace and call `EncodeBindValue` / `ResolveBindVar`. The `fmt` and `sqltypes` imports are removed from this file because the encoding and error-formatting logic they existed for has moved with the bodies onto the formatter; `bytes` and `querypb` remain because they are still referenced here. This cluster keeps every public function name and signature intact for callers (notably the vtgate/vttablet substitution paths and the test helpers that exercise bound generation), so the change reads as "the buffer now owns bound construction" rather than as an unrelated contract change. Production role: the parsed-query entry point still accepts the same inputs and returns the same outputs, but only by asking the formatter to do the substitution.

### `redact_query.go` — the redaction entry point delegates

`RedactSQLQuery` no longer keeps its own throwaway bind-variable map, calls `Normalize` inline, or concatenates margin comments itself; instead it builds a `TrackedBuffer` via the standard constructor, chains `SetBindVars(...).SetRedactPrefix("redacted")`, calls `buf.WriteRedacted(...)`, and returns `buf.String()`. The redaction concern's body — the `Normalize` call and the rendered, scrubbed statement — now lives on the formatter; this file keeps only the entry-point plumbing and the import it still needs. Production role: safe-for-display query string production, reached by delegating to the formatter that now owns literal scrubbing. The marginal-comment framing and the `"redacted"` prefix are preserved as configuration handed to the formatter.

### `impossible_query.go` — the shadow entry point delegates

`FormatImpossibleQuery` collapses from its select/union/default switch to a single `buf.WriteImpossibleQuery(node)` call. The whole shadow-emission body, including the `1 != 1` where clause and the group-by passthrough, moves onto the formatter. Production role: producing an impossible statement through the formatter that now owns shadow emission, leaving this file as the documented entry point whose doc comment (the vtgate/vttablet rationale) is retained.

## Notes on scope

Other rendering-adjacent code in the package (the `sqltypes` value encoders and the per-node `Format` methods) was not folded onto the formatter: they render individual values or nodes rather than producing a query-emission concern, and moving them would import a different smell rather than extend this one. The shared mutable `bindVars` field and the `redactPrefix` field were placed on the formatter deliberately, because threading the state through the accumulator is what makes the absorbed responsibilities interleave with (and depend on) the formatter's own rendering state, rather than being cleanly separable side helpers.
