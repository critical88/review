# Change record: shared session-store contract for Mesop's server-side stores

## Maintenance motivation

Mesop currently has two independent, session-scoped server stores that grew at different times:

1. **State session backends** (`mesop/server/state_session.py`) — the optional server-side state cache that lets clients skip re-sending full state on every event. Five implementations (null, memory, file, Firestore, SQL) sit behind a small protocol, and a config-driven factory picks one at import time into a module-level singleton. Each backend evolved its own private vocabulary: the file backend has a class-local token regex, the persisted backends share two module-level dataclass codec helpers, and token minting for the restore flow lives in an unrelated module (`mesop/server/server_utils.py::generate_state_token`).
2. **Pending-cookie redemption** (`mesop/server/server.py::` class `_CookieTokenCache`) — a small store that lets the server queue browser cookies for the client to apply, redeemed over a signed one-time nonce via the `/__apply-cookies` route. It mints its own nonce inline with `secrets`, its eviction policy is embedded inside `pop`, and it is completely unconnected to the state-session machinery.

The friction a maintainer would legitimately run into: every store handles the same shape of problem — mint an unguessable token, admit it, store a payload, hand it back exactly once, and keep the store from growing without bound — yet each one re-derives those steps privately. New contributors asking "where do I add a store that survives hot reloads?" have no single place to look, and the token-handling rules scattered across two file-local regexes, a `secrets` call buried in a server route, and a module function in `server_utils` keep getting re-implemented from scratch whenever a store is added.

## The development evolution this change models

The change is the kind of consolidation commit a framework maintainers' list commonly suggests after two parallel features (server-side state sessions, then pending-cookie application) are both in production: factor the duplicated token/store life-cycle steps into a single shared contract for "session-scoped server stores", give the reference implementations a common base so each new store inherits the boring parts, and re-point the existing public-ish seams at the unified vocabulary. No user-facing behavior is intended to change; the seams being re-pointed are internal.

As usual for a consolidation commit, the diff contains a few incidental edits that ride along (import reordering, removal of a now-unused helper, a factory return-type annotation) as well as the core re-basing work. Reviewers evaluating this change will disagree about how much the two stores really share; the notes below record what changed and why each site took the shape it did so that disagreement can be grounded in specifics.

## Overall design

- A new module, `mesop/server/session_backend.py`, becomes the home for the shared store contract: a `SessionBackend` Protocol declaring nine verbs (state persistence `restore`/`save`, housekeeping `clear_stale_sessions`, token minting `issue_token`, token admission `validate_token`, the state codec `encode_states`/`decode_states`, and one-time redemption `put`/`pop`), the `States` type alias, and the token-alphabet regex used for admission checks.
- A `BaseStateSessionBackend` template base declares the state verbs abstract and hands the persisted backends concrete defaults for minting (16-byte URL-safe token), admission (alphabet check), and the dataclass codec (serialize on encode, field-wise update on decode).
- The five state backends are re-based: the null backend adopts the protocol directly, the four persistent backends adopt the template base. The file backend's former private token check becomes the shared `validate_token` predicate; the persisted backends call the codec through `self` instead of through module-level helpers.
- The pending-cookie store is re-based directly onto the protocol so both stores answer to the same shape; its true duties (`put`/`pop`) remain its own, and its nonce eviction gains a first-class method.
- The state-token minting site (`create_update_state_event`) now asks the singleton to mint rather than calling the free function in `server_utils`, so `generate_state_token` is deleted along with its `secrets` import.

## Per-cluster notes

### `mesop/server/session_backend.py` (new file)

*What:* new module declaring the `SessionBackend` Protocol (nine verbs), `BaseStateSessionBackend`, the `States` alias, the token-alphabet regex, and the dataclass codec helpers imported from `mesop.dataclass_utils`.

*Why this shape:* a new module is structurally forced here — the contract must be importable by both `state_session.py` and `server.py` without either importing the other. Putting the protocol in `state_session.py` would have made `server.py` import `state_session` (and transitively `mesop.server.config` and SQLAlchemy/Firestore wiring) just to type-check a cookie store; putting it in `server.py` would have inverted the dependency. The base class co-locates the "template method" defaults with the protocol they belong to, and the `PendingCookie` forward reference is kept under `TYPE_CHECKING` so the annotation of `put`/`pop` does not create an import cycle with the runtime context.

*Production role:* the single canonical place future stores would implement; the token alphabet definition referenced by admission checks.

### `mesop/server/state_session.py`

*What:* imports switched from the local protocol to the shared module; the old `StateSessionBackend` protocol and `States` alias are removed in favor of the re-housed versions; each backend's base is exchanged (`NullStateSessionBackend(SessionBackend)` directly; the four persisted backends on `BaseStateSessionBackend`); the token regex class attribute is removed from the file backend in favor of the inherited predicate; restore/save bodies switch their `_serialize_state`/`_deserialize_state` calls to `self.encode_states`/`self.decode_states`; the factory's return annotation widens to `SessionBackend`; and the two module-level codec functions are deleted.

*Why this shape:* the null backend is left on the protocol rather than the template base on purpose — it is the "no state sessions configured" sentinel, and its whole contract is to refuse restore and no-op the rest; giving it concrete minting/codec defaults would imply capabilities that disabled deployments will never exercise, so protocol stubs state that honestly. The persisted backends, which all store serialized payloads, take the template base and drop their private copies of logic that only differed by accident. The codec becomes interface verbs because "how a state turns into a storable payload and back" is store concern, not module concern — Firestore and SQL already round-trip through the same pair, and keeping them private-but-duplicated invited drift. The factory annotation change is a ride-along from the protocol rename; the singleton line itself is untouched.

*Production role:* the five real store implementations plus the config factory; the module's public import surface (backends, `States`, the factory, the singleton) is kept stable for callers and tests.

### `mesop/server/server.py`

*What:* the `_CookieTokenCache` class header changes from a plain class to a `SessionBackend` implementor; seven methods are added ahead of its real `put`/`pop`: `restore`, `save`, `encode_states`, `decode_states` (all refusing, each raising `MesopException`), `validate_token` (truthiness admission), `issue_token` (URL-safe nonce), and `clear_stale_sessions` (a locked eviction sweep that folds in the nonce cleanup pop already did inline); a new import of the shared contract module joins the existing mesop imports; and the exception import is regrouped into parenthesized form to match the import-order rules the file now needs.

*Why this shape:* the refusals follow the file-local precedent for "wrong store for this operation" errors that Mesop already uses in exactly one place per failure mode — rather than raising bare `NotImplementedError`, they spell out that the cookie token cache does not restore application state, so a misconfigured future caller gets a diagnosable message instead of a protocol stub. `issue_token` exists because minting is part of the store contract now, and it is exactly the same `secrets.token_urlsafe(16)` the class already used internally, now named. The housekeeping verb is real rather than a `pass`: nonce bookkeeping needs boundedness regardless of the contract, so the eviction logic that previously lived inline in `pop` is given a name of its own — the class answered "should this store be sweepable?" with yes, independently of the state-session TTLs.

*Production role:* the pending-cookie store reachable from `maybe_append_apply_cookies_command` (enqueue at render) and the `/__apply-cookies` route (redeem once); its two-personality nature — a store for browser cookies that happens to dispatch tokens — is precisely what makes the unified contract convenient to declare against.

### `mesop/server/server_utils.py`

*What:* `generate_state_token()` is deleted together with its now-only user (`secrets` import), replaced in `create_update_state_event` by `state_session.issue_token()`, with the singleton imported from `mesop.server.state_session`.

*Why this shape:* token minting for state restoration is a store life-cycle step; with minting on the contract, the free function is a bypass around the abstraction a consolidation commit is trying to establish. The edit is intentionally minimal — the minting call remains gated behind `app_config.state_session_enabled`, so no token is minted for disabled deployments, and the emitted token's form is identical to the deleted helper's.

*Production role:* the only production minting site for state tokens; events keep carrying a `state_token` field exactly when state sessions are enabled.

### Deliberate structural variation, recorded as design

The contract reaches each implementor through a genuinely different carrier, and the notes are kept here so a later reader can audit whether each is justified:

1. **Direct protocol inheritance** (null backend) — inherits stub bodies that raise, keeping the disabled-sessions sentinel honest.
2. **One-level-deep template base** (persisted backends) — minting/admission/codec defaults arrive via `BaseStateSessionBackend`, so the inheritance edge to the contract is not visible on the decorated class and understanding it requires reading the new module.
3. **Refusal implementations** (cookie store's state verbs) — meaningful-looking method bodies that can only ever raise, in the class's house exception style.
4. **Genuine internal reuse kept for one sibling** (file backend's path guard becomes `validate_token`) — the predicate is real code on the one backend whose tokens touch the filesystem, while the equivalent-named predicate on the sibling backends is never consulted, because each store derives admission from its own lookup result.
5. **Consumer re-routing** (`server_utils` minting) — the semantics of a change of this kind are not visible from any single file; the minting site, both singletons, the factory, and the new module all participate.

This spread mirrors how contracts in long-lived server code actually accrete: nothing here appears in one canonical shape, and an audit of the change naturally has to reason about runtime dispatch (which store can actually be behind each singleton when each route executes) rather than about textual patterns.

## Scope boundaries, kept deliberately

`mesop/runtime/context.py` (the restore/save consumer), the WSGI teardown sweep, and every test/example file are left untouched: the consolidation intent was "one contract, existing call sites", and the context paths already speak to the stores exclusively through the singletons. The singletons' construction lines are byte-identical to before, which keeps import-order behavior for downstream Flask wiring stable.
