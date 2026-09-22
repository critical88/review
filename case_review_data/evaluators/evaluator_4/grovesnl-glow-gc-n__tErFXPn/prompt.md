# The native context is doing everyone's homework

I went to fix what looked like a one-line parsing bug last week and
spent forty minutes just finding out where version strings are actually
parsed on the native path: the version value's parse entry turns out to
forward into the backend context type, so the grammar that produces the
value lives in the context while the value itself - and all the tests
describing it - live in a small module of their own.

Once I noticed that, I kept finding the same pattern everywhere I looked
in the native backend. Creating a context from a loader function no
longer just loads the driver: construction also parses the raw version
string, runs two different extension-discovery routines (picking between
the indexed walk and the legacy split answer), records the device's label
length budget, and takes custody of the boxed debug callback pointer, so
teardown has to go through an internal release helper of its own. The
capability probes - including the one deciding whether debug output is
supported - read a private collection that only construction ever fills.
Label queries silently depend on a caching decision made at creation
time. And the crate root has grown a second impl block for the context
so that pixel-buffer sizing questions can also be asked through the
context handle, right next to the free helpers that already answer them.
Meanwhile the thing the context genuinely is - a few device fields and a
table of loaded function pointers - is buried underneath all of that.

This state makes ordinary work harder in concrete ways: nobody can tell
which piece of state is authoritative after construction, touching
version behavior requires reasoning about the backend type (and vice
versa), and every new device question tends to land on the context
because that is where all the previous ones ended up.

We would like the context to go back to wiring OpenGL up, with each of
these jobs in the hands of the component that naturally owns it: parsing
grammar (and its quirks) back with the version value it produces;
extension facts kept wherever they conceptually belong instead of a
context-held collection; the label limit honored at its point of use
rather than cached as ambient context state; callback install and
teardown self-contained again with the logic that registers callbacks in
the first place; sizing math in one place only. Please investigate the
native backend and everyone it now reaches into, and hand back every
responsibility you find absorbed this way - not just the first one.

None of this may change observable behavior: every version-string claim
in the version test module must parse to the same value as before
(including the embedded and WebGL mappings), devices without the debug
extension must behave exactly as before including label queries, an
installed callback must still be torn down exactly once and only when
installed, sizing answers must equal what the crate-level helpers return
(and those helpers must keep working for their existing callers), and the
public API of the crate must not change. The full test suite must be
green when you are done.
