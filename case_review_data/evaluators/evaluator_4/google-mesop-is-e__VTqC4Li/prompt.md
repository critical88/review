# Refactor request: the session-store contract overreaches

## Maintainer observation

While reviewing our server-side store code we adopted a common "session store" contract for everything that hands out tokens server-side: the pluggable state-session backends (`mesop/server/state_session.py`), the pending-cookie redemption cache (`mesop/server/server.py`), and the token minting in `mesop/server/server_utils.py`. The consolidated contract advertises every store operation — state persistence (`restore`/`save`), housekeeping sweeps, token minting, token admission checks, the dataclass state codec, and one-time cookie redemption (`put`/`pop`) — to **every** store, regardless of what that store can ever actually do.

That was a mistake. Concrete stores all over this layer now depend on, and provide, capabilities that no reachable call path dispatches on them: cookie-related obligations live on every state backend, state persistence and serialization obligations live on the cookie store, and token helpers are spread onto stores that never mint or validate tokens themselves. Several of these members are dead weight by construction — refusal bodies and placeholder implementations that can never execute — and the widest interface has become the default dependency for future maintainers.

## Responsibility area

The problem is the contract itself in `mesop/server/` (with the shared interface module adopted by the state-session backends), not any single backend's implementation. The two real consumers of these stores exist today and must keep working:

- the runtime context's state restore/save path, driven through the config-selected state-session singleton;
- the browser-cookie flow — `maybe_append_apply_cookies_command` enqueues pending cookies, and the `/__apply-cookies` route redeems them once.

Anything else the shared contract advertises to a store should be understood as suspect: ask for each capability whether any production call site can reach it on that concrete store, and reshape accordingly.

## Desired outcome

Reshape this layer along the actual consumers so that:

1. Each concrete store implements and depends on only the capabilities that reachable production call paths actually invoke on it. The state persistence contract and the one-time cookie redemption contract have different consumers and different lifecycles; models that are cohesive for one side do not fit the other.
2. The template-style base for persisted state backends keeps only what those backends genuinely share — token minting, admission, and serialization behavior belong where they are exercised, or on a contract narrow enough that every implementor can genuinely use them.
3. No placeholder, stub, refusal, or unreachable method remains merely to satisfy a wide interface; each declared member should have a reachable dispatcher. If a store does not need a capability, it should not declare it or inherit it.
4. Members that genuinely serve production — e.g. the file-backed store's path-admission guard, the nonce bounds for the cookie flow, backend-specific token minting — are preserved in behavior, whatever their new home.
5. Leftovers of the consolidation that no remaining store or consumer uses (types, declarations, imports, helpers that became purposeless) are cleaned up rather than left behind.

## Compatibility boundary — behavior and API that must stay stable

- The per-backend state semantics are unchanged: restore-then-consumption flows, miss handling and messages, TTL/GC behavior per backend, request-count gating for the SQL backend, the disabled-sessions no-op behavior, and one-restore-per-token behavior.
- State-session stores are still constructed exactly as before, configuration keys and the factory behavior are unchanged, and whatever store the factory returns still answers the runtime context's restore/save calls.
- `create_update_state_event` still emits a state token exactly when state sessions are enabled, with tokens of the same random form, and emits none otherwise.
- Pending-cookie redemption remains single-use with the same signature and TTL window; replay and expired-token handling still reject; the `/__apply-cookies` route and the render-time enqueue keep working.
- The file-based store still rejects non-conforming tokens before touching the filesystem.
- The public seam the test suite relies on stays importable: the backend classes, config factory, singleton, and `States` type from their existing module.
- The complete existing test suite keeps passing, unmodified and without deletion of test coverage.
