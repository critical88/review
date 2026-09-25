# Injection design record — gspread 6.0 migration leftovers

## Realistic maintenance motivation

`gspread` 6.0 (commit `7ca71eaa`) consolidated three public-API surfaces, and
the retiring of each left private implementation behind:

- **HTTP transport split (PR #1190).** The HTTP/machinery that used to live in
  the monolithic `Client` was split into `gspread.http_client`, and the
  `http_client` argument of `Client`, `authorize`, `oauth`, `oauth_from_dict`,
  `service_account`, `service_account_from_dict` and `api_key` became a callable
  factory (`HTTPClientType`) rather than a named selection. The supported
  factories are `HTTPClient` and `BackOffHTTPClient`.
- **OAuth flow as a callable.** The `flow` argument became a `FlowCallable`
  (`local_server_flow`); the interactive copy/paste-the-code "console" strategy
  that the `oauth`/`oauth_from_dict` docstrings still describe was dropped
  upstream.
- **Removed deprecated API.** `Worksheet.delete_row` (PR #1062) and the
  `accepted_kwargs` keyword-aliasing decorator (PR #1229) were removed; the
  `.delete_rows` methods remain.

The modeled defect is the ordinary accident of finishing a migration without a
second cleanup pass: the private plumbing each retired surface depended on
stayed in the tree for "one more release" so downstream mirrors could keep
importing the modules, then was never removed.

## Overall design

The injection introduces five private, module-level clusters that nothing the
6.0 API exposes can reach. Each cluster stays syntactically present only through
references that never result in its code running — self-reference, a
by-name registry, a permanently disabled migration switch, or a sibling import.
A naive name walk therefore reports several of these as "used" until the
holding reference is itself removed. The clusters live alongside the real 6.0
implementations and import only stdlib or already-imported names, so the public
behavior and the test suite are untouched.

## Per-cluster design rationale

### `gspread/auth.py` — retired console OAuth flow and its by-name lookup

`_console_flow` (line 410) is a transcript of the dropped interactive flow: it
builds an `InstalledAppFlow` and calls `flow.run_console()` (annotated
`# type: ignore[attr-defined]` because that method no longer exists on the
upstream `InstalledAppFlow`). It models the function the `oauth`/`oauth_from_dict`
docstrings still reference.

`_LEGACY_FLOW_REGISTRY` (line 427) is a mapping that the pre-6.0 `flow="..."`
argument used to dispatch through. It binds `"console"` to `_console_flow` and
`"local_server_flow"` to the real `local_server_flow`, so the retired entry is
held in place by the real flow it sits next to.

`_legacy_flow_by_name` (line 433) is the lookup helper that `oauth(flow="...")`
consulted. After the argument became a `FlowCallable` it lost every caller; it
indexes `_LEGACY_FLOW_REGISTRY`, keeping the registry syntactically alive. This
is the most internally self-referential cluster — helper, registry and entry
form a ring that nothing outside the ring enters.

`_console_flow` was placed before `_LEGACY_FLOW_REGISTRY` so the registry's dict
literal can name it, and `_LEGACY_FLOW_REGISTRY` before `_legacy_flow_by_name`,
matching the order an ordinary refactor would leave behind.

### `gspread/http_client.py` — by-name engine registry, dispatcher, and the always-off switch

After `HTTPClientType = Type[HTTPClient]`, the injection adds the retired
by-name selection layer:

- `_LEGACY_LOGIN_ENGINE_COMPAT_ENABLED = False` (line ~) is a private
  migration switch assigned exactly once to `False`. It represents a
  one-release compatibility gate that was never flipped back on; the constant
  stays live because its own `if`-condition reads it, while the body it guards
  is dead.
- `_InteractiveLoginEngine(HTTPClient)` is a transcript of a retired
  console login engine with `_apply_legacy_api_key_inline` and `login_and_block`
  methods. The class is held live by the registry literal below; it becomes
  unreachable only once that registry is gone.
- `_LEGACY_CLIENT_ENGINES` (line 691) maps `"backoff"` to `BackOffHTTPClient`
  (the real supported factory) and `"interactive"` to the retired engine — a
  registry holding one live entry placed beside the retired one, so the retired
  entry looks used.
- `_apply_legacy_api_key_param` is a helper the retired engine calls; it stays
  live while the engine does.
- A guarded branch `if _LEGACY_LOGIN_ENGINE_COMPAT_ENABLED:` registers a default
  engine. Because the switch is permanently off, this body never runs, but it is
  the only remaining textual reference to the registry from outside the helper
  that indexes it.

This is the cross-module center of the injection: the dispatcher in
`gspread.client` imports `_LEGACY_CLIENT_ENGINES`.

### `gspread/client.py` — cross-module dispatcher using a dead decorator

After `Client.remove_permission`, the injection adds `from .utils import
_accepted_kwargs` (a sibling-module import) and `_resolve_legacy_client_engine`
decorated `@_accepted_kwargs(engine="name")`. Its body imports
`_LEGACY_CLIENT_ENGINES` from `http_client` and calls
`factory = _LEGACY_CLIENT_ENGINES[name]; return factory(auth, session)`. This is
the retired by-name resolution that `authorize(http_client="backoff")` used to
go through; the public argument became a callable factory and this lost every
caller. It is the cross-module seam: it imports the dead http_client registry
and is decorated by the dead utils decorator. Removing it strands both the
import and the decorator's use, which is the dependent-cleanup the case demands.

### `gspread/utils.py` — keyword-alias shim and deprecation warning helper

After the `if __name__ == "__main__":` block (and the new `import warnings`),
the injection adds:

- `_LEGACY_KWARG_ALIASES` (line 1176), a dict mapping legacy keyword names to
  canonical ones that the removed `accepted_kwargs` decorator consumed. It is
  referenced by nothing; it survived the removal of `accepted_kwargs` because
  nobody noticed it was only ever read by the decorator.
- `_accepted_kwargs` (line 1184), a reconstruction of the removed decorator: it
  wraps a function, pops legacy kwargs and emits a deprecation warning before
  forwarding. It is referenced only by the dead dispatcher in `client.py`
  (imported there and applied as a decorator) and by its own internals.
- `_legacy_deprecation_warning` (line 1209), the helper `_accepted_kwargs` calls
  to emit a `DeprecationWarning`.

This lets the dispatcher carry the 5.x keyword-alias shim exactly as pre-6.0
transport glue did, which is why the dispatcher is decorated by the dead
decorator rather than called plain.

### `gspread/worksheet.py` — low-level request builder for a removed method

At the end of the file, `_legacy_delete_row_request` (line 3706) builds the
`deleteDimension` request dict that the removed `delete_row` method posted. It
references no other injected symbol and no caller remains; it is the plainest
orphaned helper. It is placed after the `Worksheet` class closes so it does not
nest inside the class.

## Site and shape selection

Each cluster was placed beside the real 6.0 implementation it supported, uses
only names already in scope (`Callable`, `wraps`, `T`, `warnings`, the existing
factory classes, `InstalledAppFlow`), and mirrors the genuine retired
functionality (the real docstrings still describe `console_flow`; the real 6.0
factories are `HTTPClient`/`BackOffHTTPClient`; the real removed method was
`delete_row`). The switch is exactly the shape a one-release deprecation gate
takes — a private constant assigned once to `False` — so that a guarded body
reads as intentional compat code rather than dead code. The registries keep a
live ("real") entry beside each retired entry so a name walk over-reports the
retired entries as used, which is the central non-mechanical property: a
context-free "no references" pass removes only the leaves, and the holding
registries and switches only become unreachable once those leaves are gone.
