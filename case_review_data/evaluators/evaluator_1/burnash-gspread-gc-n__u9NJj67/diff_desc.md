# Injection design record — gspread (pinned revision 7ca71ea)

## Maintenance motivation

`gspread` is a small library with one famous promise: authenticate once, hold a
single client object, and drive every spreadsheet operation from it. The client
entry object (`gspread/client.py`, class `Client`) is therefore the library's
most visible surface and the natural gravity well for feature requests. Issues
and PRs of the kind this record models appear repeatedly in real client-library
histories:

- "it's confusing that authentication helpers live in a separate module while
  everything else is a method on the client";
- "I want autocomplete to show me how to log in, not have to read the docs to
  find a second module";
- "why do I have to know about the HTTP layer at all — the client should just
  be able to send requests itself";
- "open-by-key is two steps; give me a one-liner that reads or writes a range
  directly from the client".

Each request is individually defensible, and each lands where discovery is
easiest: on the entry class. The modeled contributor experience is someone
repeatedly choosing the path of least resistance for API ergonomics, with no
single change that a reviewer would call unreasonable on its own.

## Modeled evolution

The change models four consecutive, realistic development steps, in the order
a maintainer would most plausibly accept them:

1. **Transport absorption by inheritance.** The client is made a subclass of
   the HTTP layer so that "the client is a request sender" and subclass users
   of the transport keep working without knowing about the composition. The
   existing composed transport attribute is deliberately kept, because dozens
   of existing methods already route through it; the merge is presented as
   an additive convenience rather than a rewrite.
2. **Authentication surface hoisted onto the class.** The OAuth helpers that
   lived in `gspread.auth` (local-server flow, credential file load/store,
   the config-directory default, and the flow-callable protocol) are copied
   onto the client as static/class methods, and the module functions are
   reduced to delegating shims "for backwards compatibility". The module
   constants (`DEFAULT_SCOPES`, `DEFAULT_*_FILENAME`, ...) turn into class
   attributes, re-exported by the module.
3. **Credential conversion pulled in.** The oauth2client→google-auth
   conversion helpers that lived in `gspread.utils` move next to the
   authentication factories that call them, and the utils module keeps a
   delegating function so existing importers do not break.
4. **Direct by-key range accessors.** Two convenience methods are added so a
   user can read or write a value range from a spreadsheet ID without opening
   the spreadsheet first, each building the values URL itself in the same
   style the transport layer already uses.

The end state is a class that simultaneously owns transport state (in two
shapes), the whole credential lifecycle (acquire, persist, load, convert),
factory entry points for every authentication method, duplicated endpoint
building for the Sheets values family, and its original coordination duties
(open/create/list/copy/delete, permissions, comments), while the two helper
modules that previously owned those concerns survive only as forwarders into
it.

## Overall design

Three production files participate:

- `gspread/client.py` grows the extended entry class (transport base, class
  attributes for credential defaults, credential lifecycle methods, credential
  conversion helpers, authentication factory methods, by-key range accessors,
  and the relocated flow-callable protocol).
- `gspread/auth.py` keeps every public name and docstring but its functions
  become one-line delegations into the entry class, and its constants become
  re-exports of class attributes.
- `gspread/utils.py` loses the implementation of the credential conversion
  helpers and keeps a single delegating function with a lazy import of the
  client module.

Design decisions an auditor should weigh:

- **Inheritance plus composition, deliberately coexisting.** The class
  inherits the transport's request machinery and session state, while
  `__init__` still constructs an inner transport instance exactly as before
  and every pre-existing method keeps routing through it. This is how such a
  merge realistically lands in a library with an installed base: the live
  request path is unchanged, so users who subclass the HTTP layer or inject a
  custom transport class observe no difference. The cost is that volatile
  transport state now exists twice per object, once inherited and once
  composed, which is a state-synchronization hazard the original design did
  not have.
- **Credential helpers as static/class methods with import-time defaults.**
  Mirroring the module-level pattern (defaults computed once at import time
  and exposed as class attributes) was the lowest-friction way to move them;
  it preserves the "constants are already computed for you" behavior that
  docstrings advertise.
- **Module facades instead of removal.** `gspread.auth` and
  `gspread.utils.convert_credentials` are widely imported entry points
  (including by the package's own top-level exports and by downstream code),
  so the model keeps every public name working rather than deleting them.
  The dependency direction between the modules inverts: the auth module now
  depends on the client module it used to be independent of.
- **Lazy import in the utils shim.** The client module imports utils at
  module level, so a module-level reverse import would be circular; the
  delegating function therefore resolves the client module at call time.
  This is a realistic compat shim shape, and it leaves the utils module
  depending on the entry class for a core credential concern.

## Changed locations

### Cluster A — `gspread/client.py`: the client entry class

**A1. Module imports and class header.** The class now derives from the HTTP
layer class and gains the imports that the absorbed concerns need (credential
types from `google.oauth2` and `google.auth`, the interactive OAuth flow class
from `google_auth_oauthlib`, path/json/os primitives for credential file
handling, and the values-family URL constant and quoting helper for the new
range accessors). *Why this shape:* deriving from the transport keeps
`isinstance` checks and any subclass users of the transport working, which is
the least disruptive way to declare "the client can send requests itself".
*Production role:* makes the entry class the owner of the request path in
addition to the coordination duty it already had.

**A2. `__init__` keeps constructing the composed transport.** The constructor
still instantiates the injected transport class exactly as before; the
inherited transport state is initialized by the base class. *Why this site:*
changing the constructor would break the recorded usage
`Client(auth=..., session=..., http_client=<custom class>)`, so the merge
keeps both lifecycles alive side by side. *Production role:* the entry class
now holds two transport lifecycles — a newly added inherited one and the
composed one every existing method uses.

**A3. Credential-lifecycle class attributes.** Default scope lists, the
API-key availability flag, the config-directory static method, and the three
default credential file paths (computed from the config directory at class
creation time) move here as class attributes/static methods. *Why here:* these
defaults belong to the authentication story the class now tells, and computing
them as class attributes mirrors the import-time constants they replace.
*Production role:* the class becomes the discoverable source of the library's
credential defaults; the auth module re-exports them.

**A4. Interactive-flow execution and credential persistence** — the
local-server flow runner (creating an `InstalledAppFlow`, running it, returning
the resulting credentials), the authorized-user file loader, and the
credentials-to-disk writer (mkdir parents, write `to_json` output). *Why these
shapes:* they are verbatim relocations of the auth-module implementations,
converted to static methods, because the modeled change is a move for
discoverability, not a redesign. *Production role:* acquiring and persisting
OAuth user credentials is now a service of the entry class.

**A5. The flow-callable protocol** moves here from the auth module (a typing
`Protocol` describing what a custom flow callable must accept and return).
*Why:* the factory methods now reference it, and keeping it next to them
avoids a back-import from client to auth. *Production role:* the contract for
pluggable OAuth flows is now defined by the entry class.

**A6. Credential conversion helpers** — a classmethod dispatching on
credential type to two static helpers reconstructing OAuth credentials and
service-account credentials from oauth2client objects. *Why here/shape:*
these helpers exist to support the authentication factories, so the modeled
step places them right next to those factories on the class; the utils module
forwards to them. *Production role:* the oauth2client compatibility concern —
formerly a utils concern — is now reached through the entry class, including
from within the utils module itself (lazy import).

**A7. Authentication factory methods** — classmethods for credential-based
authorization, OAuth with stored/reloaded user files, OAuth from dicts,
service account from file, service account from dict, and API-key auth, each
ending in instantiating the client class itself. *Why classmethods:* users are
meant to call them on the class, and classmethods let a subclass inherit the
factories. *Production role:* every way of constructing this class now
originates on the class itself, cementing class-level coupling — factory,
credential lifecycle, and default paths in one place.

**A8. By-key range accessors** — a reader and a writer that take a spreadsheet
ID plus A1 range, build the values URL themselves (with the quoting helper),
add default row-dimension params / default raw input option, and issue the
request through the composed transport. *Why duplicated rather than
delegated:* the modeled contributor wanted a "no-spreadsheet-object shortcut"
and copied the URL-building idiom from the transport layer rather than
extending it. *Production role:* endpoint knowledge for the values family now
exists in a second owner inside the same package.

**A9. Pre-existing coordination methods unchanged.** The open/open_by_key/
open_by_url/openall/create/copy/del_spreadsheet/export/import_csv/
list_permissions/insert_permission/remove_permission methods (Drive listing
with pagination, permissions, comments copying, building `Spreadsheet`
aggregates) keep their bodies and keep routing through the composed
transport. *Why untouched:* the modeled evolution is additive; the new
concerns are interleaved around — not replacing — the class's original
coordination duty, which is what makes the end state read as one over-grown
class rather than a rewrite.

### Cluster B — `gspread/auth.py`: facade reduced to forwarders

**B1. Module imports and constants.** The flow/credential imports the
implementations needed are removed; the module now imports the client class,
and `DEFAULT_SCOPES`, `READONLY_SCOPES`, `DEFAULT_CONFIG_DIR`, and the three
`DEFAULT_*_FILENAME` constants become assignments from class attributes.
*Why:* the module must keep exporting every documented name (including via
`gspread`'s top-level re-exports and docstring references) while ceding
ownership to the class.

**B2. Every public function becomes a one-line delegation** into the entry
class: config-dir computation, credential authorization, local-server flow
execution, credential load/store, the OAuth flow (which still returns the
constructed client), dict-based OAuth, the two service-account constructors,
and API-key authentication. Docstrings are preserved verbatim. *Why this
shape:* it is the cheapest way to move behavior while keeping every import
stable — each function forwards keyword arguments to the corresponding class
attribute. *Production role:* the authentication module no longer contains a
single independent decision; the package's auth story now has exactly one
implementation site, the entry class, and this module is its compatibility
facade.

### Cluster C — `gspread/utils.py`: conversion leaves the module

**C1. Credential conversion.** The dedicated oauth2client import block and
the three conversion function bodies (the type-dispatching entry point and
the OAuth/service-account converters) are removed; the public entry point
survives as a delegating function that lazily imports the client module and
calls the classmethod that now owns the logic. *Why a lazy import:* the
client module imports this module at import time (URL/label helpers), so the
reverse dependency can only be resolved at call time — the standard shape of
a compat shim in a circular-import situation. *Production role:* the utils
module, which the HTTP layer depends on, now reaches back into the entry
class for a core credential concern, adding a package-internal dependency
edge that did not exist before.

## Deliberate non-changes

The test suite, the HTTP layer module, the spreadsheet/worksheet/cell domain
modules, and the URL constants module are not part of this change: the
absorptions were all designed to preserve the live request path, the
transport's retry behavior, and the public import surface of the package, so
that the recorded suite continues to exercise the same behavior in the same
way. Whether the resulting structure is acceptable, and how the pieces should
be re-separated, is a separate question from the one this record documents.
