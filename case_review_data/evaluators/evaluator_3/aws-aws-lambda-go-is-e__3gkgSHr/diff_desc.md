# Injection design record — one unified capability surface for the Lambda runtime integrations

## Maintenance motivation

The aws-lambda-go library lets a deployment run the same user handler in two very
different execution modes: the modern provided-runtime API loop that talks HTTP to
the Lambda Runtime API, and the deprecated go1.x net/rpc mode that is still
required for the `lambda.norpc` builds and for external runtimes that register the
public function type. Those two paths had each evolved their own way of consuming
a handler: the loop reached through the concrete options holder and dispatched the
wrapped stream-returning function value, while the RPC mode read the configured
base context directly off the same options holder and dispatched the serialized
invoke.

The maintainer story modeled here is a plausible—but ultimately regrettable—attempt
to make that asymmetry systematic: *give the handler contract one explicit,
uniform capability surface, and make every runtime component consume handlers
only through that surface.* The same doctrine is applied a second time to the HTTP
integrations: instead of each client specifying only what it does with a transport,
the transports are described through a single mirrored surface matching the full
convenience verb set that a standard library client offers, so that "transports,
mocks, and wrapping round trippers are interchangeable everywhere the runtime talks
over HTTP."

## Normal development evolution modeled

This is the shape of a large "make the contracts uniform" refactor that a team
lands in one piece because no step ever leaves a declaration, a consumer, or a
double unsatisfied:

1. the public handler contract is rewritten as one capability list, with per-
   capability documentation;
2. the adapter that wraps function values grows the methods required to keep
   satisfying that list, so existing wiring keeps compiling by promotion;
3. consumers (the API loop and the RPC function mode) are re-typed from the
   concrete options holder onto the contract;
4. where a consumer used to read composition state directly (the configured base
   context), a capability accessor is introduced so the read can happen through
   the contract;
5. a narrow private alias is added where the widened public contract would have
   broken the documented back-compat acceptance path for legacy handlers;
6. the HTTP transports get a named mirror of the standard client's request
   surface, and both API clients plus the third-party response path adopt it;
7. the entry-point documentation and the package's own test doubles are updated
   to describe and satisfy the new surfaces;
8. the pipeline that types and releases *responses* is left untouched, because it
   already serves those responsibilities for returned response objects.

Every step is the kind of edit a real refactor of this scale produces, and each
one is forced by the widening in the step before it: once the contract grows,
implementors must grow stubs; once consumers are re-typed, direct reads must
become capability accessors; once the back-compat path breaks, a seam must absorb
it; once test doubles stop compiling, their stubs grow too.

## Overall design

Two capabilities of the design are worth stating before the per-location details,
because they shape how the change sits in the repository:

- **Carrier reach through promotion.** The function adapter that wraps user
  function values is embedded in the options holder, so methods declared on the
  adapter become part of the holder's method set. The widened surface is
  therefore satisfied by existing wiring with no changes at the construction
  sites—the capacity that makes this refactor *easy to land* and easy to
  underestimate.
- **The response pipeline is deliberately untouched.** Response typing
  (application/json versus raw bytes) and post-send release already live where the
  runtime consumes *returned response values*—the loop probes each returned
  object for a content-type method and for `io.Closer`, and the serialized path
  calls `Close` on the pooled buffer to return it to its pool. The handlers that
  are *sources* of responses are therefore not the components that serve those
  two responsibilities, and the refactor decided not to extend them there. The
  pooled JSON response type consequently keeps its own same-named content-type
  and release methods right next to the adapter pieces that grew theirs; two
  same-named methods with different fates sit in one file.

## Per-cluster rationale

### The unified handler contract (`lambda/handler.go`)

The contract is rewritten from its historical single capability—the serialized
invoke that has always been its documented requirement—into an explicit
capability list: serialized dispatch, streaming dispatch, base-context
derivation, response content typing, and handler lifecycle release, each with a
doc comment explaining its role in "the complete invocation pipeline in one
place." This is the load-bearing declaration of the whole refactor: it defines
the surface that everything downstream is expected to consume uniformly.

Two capabilities on that list are illustrative of the doctrine being applied:

- *content typing*: the runtime does consult response objects for a content type,
  but it does so through the response probe described above. Nothing dispatches
  a *handler* value for its content type, so this capability exists on the
  contract to make the surface complete rather than to serve an existing need.
- *lifecycle release*: the same is true for handler-level release. The release
  responsibility is already served for returned response objects by the closer
  probe and the pooled-buffer path; no runtime component needs handlers, as
  sources of work, to be closable.

### Carrier growth on the function adapter (`lambda/handler.go`)

The adapter gains the capabilities that keep it satisfying the unified contract
through the promotion described above:

- a streaming-dispatch method that forwards to the wrapped function value—this
  is the one new member with a real consumer downstream (the loop passes it as
  its dispatch function), and it is a genuine re-expression of the adapter's
  existing "call the wrapped handler" behavior in streaming form;
- a content-type method returning the adapter's constant JSON default;
- a no-op release method, on the reasoning that function-value handlers hold no
  state between invocations.

The first member is a real capability move; the latter two are the parts of the
list the adapter never needed before and does not need now. They are written as
straightforward implementations with doc comments describing their role in the
unified surface—the "plausible-looking default implementation" character of a
refactor executed enthusiastically rather than a stub-ification someone would
notice in review of a single method in isolation.

### Composition options as capability provider (`lambda/handler.go`)

The options holder gains a base-context accessor that returns the configured
base context, falling back to the empty background context. Its purpose is to let
consumers read the invocation parent *through the contract* instead of reading
the holder's field directly, which is what the loop and the RPC mode used to do.
It also gives the holder the missing member it needs to satisfy the unified
contract in its own right (its remaining capabilities arrive through the embedded
adapter).

### Back-compat seam (`lambda/handler.go`, handler construction)

Handler construction historically accepted *any* implementor of the exported
contract by asserting it and wrapping the serialized invoke. After the widening,
user types that provide only that one capability no longer satisfy the contract,
so the construction path is re-pointed at a new single-capability private alias
carrying the original capability. The doc comment frames it as letting "legacy
handlers keep working by asserting the capability they actually implement"—which
is precisely the situation a day-one reader should find uncomfortable: the
public surface now demands more than the private path accepts.

### Consumer threading: the provided-runtime API loop (`lambda/invoke_loop.go`)

The loop's two driver functions are re-typed from the options holder to the
contract, and the per-invoke deadline is derived from the contract's
base-context capability. The dispatch helper is changed from taking the adapter
type to taking a streaming-dispatch function value, and the loop passes the
contract's streaming-dispatch capability into it. This is the consumer-side
cost of the widening: every function in the chain now depends on the full
capability list even though its body uses, respectively, the dispatch and the
context capabilities only.

### Consumer threading: the go1.x RPC function mode (`lambda/rpc_function.go`)

The public function type's handler field is re-typed from the options holder to
the contract, and its per-invoke deadline derivation changes from the type's own
helper (which read the holder's field) to the contract's base-context
capability; the now-unused helper is removed as part of the re-typing. The
documented-for-back-compat function type is therefore also bound to the full
capability list, while its dispatch body uses the serialized invoke and the
context capabilities only.

### The mirrored transport contract (`lambda/runtime_api_client.go`, `lambda/extensions_api_client.go`)

A named transport contract is introduced, mirroring the standard client's full
request surface: prepared-request execution plus the convenience key/value
request helpers. The runtime API client's field is re-typed from the concrete
standard client to this contract, and the extensions API client adopts the
identical change. The intended benefit is maximum interchangeability of
transports and mocks; the cost is that both clients—and every future
transport—depend on two verbs no runtime integration uses, since runtime and
extensions messages need explicit request construction (per-call URLs such as
the signed runtime endpoints, headers, and bodies), not one-shot helper calls.
The standard client keeps satisfying the contract implicitly with no textual
change.

### The response delivery transport (`cfn/response.go`)

The custom-resource response path already declared a minimal one-capability
transport contract for its sender. It is widened with the same two convenience
verbs and given the same "full request surface" documentation as the runtime
transport, so the two HTTP integrations read as one doctrine. The single
consumer constructs an explicit PUT request and even deletes the Content-Type
header before sending, so the added verbs are further from a caller here than
anywhere else in the repository.

### Entry-point documentation (`lambda/entry.go`)

The two deprecated start entry points that take the public contract have their
handler-requirement documentation rewritten from "a single Invoke() function"
to the five-method capability list, matching the contract's own doc comments.
This is the refactor finishing its own paperwork: the published description of
what an implementor must provide is updated to the new doctrine. It is the first
place a user reads about the change, and the diff between the old and new
documentation shrinks the perception of the widening for anyone who starts their
review from the entry points.

### Test doubles (`lambda/rpc_function_test.go`, `cfn/response_test.go`)

The package's own doubles are where the widening stops being theory: the
handler-wrapper double used by the function-mode tests guards its
implementation of the public contract with a compile-time assignment and had to
grow streaming, base-context, content-type, and release members to keep
compiling; the response-delivery double had to grow the two convenience verbs
for the same reason, even though neither is exercised by its test. Growing the
doubles is what a refactor that wants to land quietly does: it makes the widened
surface look *accepted* by the package's own examples, rather than contested by
them.

## Bounded scope: what the refactor deliberately did not touch

To keep this an understandable single change, several adjacent regions were
excluded as a real maintainer would bound it:

- the response typing/release pipeline remains served by probing returned
  response values (the `io.Closer` and content-type probes, and the pooled-
  buffer path in the serialized execute path), including the pooled response
  type's own same-named content-type and release methods—the refactor did not
  unify those responsibilities;
- the generics-based compile-time handler validation in the generic start path
  was left as-is (it is a single-signature validation constraint, not a
  multi-party capability surface);
- the per-stage hook contracts in the handler-tracing helper were left as-is
  (they are voluntarily narrow per-stage seams for observers and were kept out
  of this dispatch-surface change);
- the two build-tagged loop entry variants still pass the concrete options
  holder into the loop, and were left compiling without re-typing;
- the extension registration/SIGTERM wiring was not given any extra capabilities;
- the events packages, the RPC wire messages, and the context payload types have
  no capability contracts and were not involved.
