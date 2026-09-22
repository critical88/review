# Change design record — a common COM handle for go-ole

## Maintenance motivation

go-ole models each COM interface as a wrapper struct around a raw vtable
pointer, and the `oleutil` convenience layer is written against one concrete
wrapper at a time: the late-binding helpers take the dispatch wrapper, the
connection helper takes the dispatch wrapper, and every other handle
(`IEnumVARIANT`, `IConnectionPoint`, `ITypeInfo`, the WinRT wrappers) has to be
re-wrapped by hand before a helper can touch it. A recurring maintenance
friction follows from this: users who already hold a valid handle keep asking
(github.com/mattn/go-ole issues and pull requests over the years) why they
must perform a `QueryInterface` dance themselves before they can call
`oleutil.CallMethod` or `oleutil.ForEach`, and maintainers keep explaining
that the helpers simply were never written against anything but the dispatch
wrapper.

The natural resolution a maintainer reaches for is the same one Go code
normally uses to accept "any of a family of things": declare an interface
that the whole wrapper family already satisfies, and re-type the helpers
against it. This change models that episode: publish one Go-level handle type
for the whole wrapper family, give every wrapper the capability surface the
handle type promises, and re-type the `oleutil` helpers so they finally
accept any handle.

## The development evolution being modeled

This change models a single, ordinary generalization step in a library that
evolves by accretion of convenience surface:

1. **Declare the abstraction.** The core package already contains exactly one
   Go interface — an identity-only three-method declaration that nothing in
   the module references. A maintainer generalizing the helper layer would
   find it and take it over rather than adding a second interface next to it:
   widen it in place into the "any COM handle" contract. Because it had no
   users, widening breaks nobody, and because it sits in the file that
   declares the root wrapper, it is the obvious publication point for what
   "a handle" means in this package.
2. **Make the promise true.** An interface method set that merely lists what
   helpers want would be fiction unless every wrapper actually provides it.
   Wrappers that own a capability natively already have the right method at
   depth 0, and Go's embedded-promotion rules give everything else nothing.
   So the base wrapper — the struct every other wrapper embeds — receives
   default implementations for the capabilities it does not natively own;
   through embedding, every derived wrapper inherits them at depth 1 while
   its own native methods continue to shadow them. That is exactly how the
   rest of this library already shares behavior across wrappers (all wrappers
   get `AddRef`/`Release`/`QueryInterface` from one place), so it is the
   idiomatic lever here.
3. **Honor the per-platform convention.** go-ole duplicates every capability
   body across a `!windows` file (error sentinels, so the library type-checks
   and CI runs on Linux) and a `_windows.go` file (the real syscalls). New
   methods on the root wrapper follow that convention: two companion files.
4. **Re-type the consumers.** `oleutil` helpers change their first parameter
   from the concrete dispatch wrapper to the new handle type. Their bodies
   mostly shrink, delegating to one interface method instead of a
   wrapper-specific convenience.

## Overall design

The design intent is a completion story that follows a normal feature arc:
promise (interface declaration) → fulfillment (base-wrapper defaults on both
platforms; capability-shaped overrides where a wrapper plausibly hosts them)
→ adoption (consumers re-typed onto the promise). Each part exists to make
the other parts defensible to a reviewer of this library:

- The interface is not abstract art; every method in it is a method name
  that already exists on some wrapper in the package, so the promise reads
  as "one common denominator of what the family can already do".
- The capability defaults are split between a **fitted** strategy (resolve
  the real interface through `QueryInterface` and delegate — how the library
  already bridges between interfaces, e.g. the enumerator accessor on
  `VARIANT` and the connection-point lookup in `oleutil`) and **sentinel**
  values where a bare handle genuinely cannot identify the target of the
  operation (advising needs to know *which* connection point).
- Whenever a wrapper falsely appears to host a capability the umbrella
  requires, the code must still run without panicking or dereferencing a
  wrong vtable slot; every new body routes through an interface query or
  returns an error, never a raw slot call outside the wrapper's own vtable.
- The consumer re-type is where the value lands for users: `oleutil`
  functions become callable with handles they already hold.

## Per-location record

### `iunknown.go` — the handle declaration

**What changed.** The unreferenced identity-only interface declaration is
replaced in place by a fourteen-method handle interface covering four
capability groups, each grouped with a comment: interface identity and
lifetime (`QueryInterface`, `AddRef`, `Release`), late binding by member name
(`GetSingleIDOfName`, `Invoke`, `CallMethod`, `GetProperty`, `PutProperty`),
element enumeration (`Next`, `Skip`, `Reset`, `Clone`), and connection-point
advising (`Advise`, `Unadvise`). The doc comment states the intent ("so
helpers can be written once against any handle") and names which wrappers
satisfy it.

**Why this site.** It is the only existing Go-level handle abstraction in the
package, located in the file that declares the embedding root; replacing (not
extending) it avoids two competing handle abstractions, and having no
existing users, the replacement is free of migration cost.

**Why this shape.** Every method name is taken verbatim from the capability
surface the wrapper family already exposes, which is what makes "all
wrappers satisfy this" credible from naming alone: the base wrapper supplies
the whole missing remainder (below), promotion does the rest, and method
resolution still prefers each wrapper's own native methods at depth 0.

**Production role.** The public parameter type of the helper layer — the
"accept any handle" promise the library hands to users.

### `iunknown_func.go` — defaults for the non-Windows build

**What changed.** Eleven default method bodies are appended for the base
wrapper: the five late-binding methods, the four enumeration methods, and
the two advising methods. Each returns a zero value plus the library's
already-standard not-implemented error construct, with the enumeration
result using an existing null-variant constructor; a comment block explains
that these defaults give every derived wrapper the handle surface and are
reached only by wrappers lacking a native capability.

**Why this site.** `!windows` companion files exist precisely so capability
declarations type-check and CI can run on Linux; a method added to the root
wrapper without a stub here would break the primary development and CI
configuration for this repository, where the recorded build and test
commands run on Linux. The bodies mirror the stub style used by every other
`*_func.go` file in the package (error sentinel, `int32(0)`, null variant).

**Production role.** Keeps the package compiling and testable on non-Windows
platforms while the handle promise exists; documents (by construction) that
the defaults which cannot reach COM machinery on this platform fail the way
the library always fails on this platform.

### `iunknown_windows.go` — defaults for the Windows build

**What changed.** The same eleven methods receive Windows bodies, mirroring
the file's existing structure. The late-binding five resolve the real
dispatch interface with the package-local `queryInterface` helper (`defer
disp.Release()` then delegate), the enumeration four resolve `IID_IEnumVariant`,
cast the returned unknown with the same `unsafe.Pointer` re-wrap used
throughout the library, and delegate. `Advise`/`Unadvise` return
`E_NOINTERFACE` with a comment explaining why: the advising capability
requires knowing which connection point to operate on, and a bare handle has
no identity for that beyond the object itself.

**Why this site.** Runtime capability for this package lives in `_windows.go`
files only; separating "what a handle promises" from "how COM realizes it"
is the codebase's split, so the fitted implementations go exactly where a
reader of this library would look for them.

**Why this shape.** The bodies reuse the two patterns the library already
trusts for crossing between its interfaces — the package's own interface-ID
query and the concrete-wrapper re-cast — instead of introducing new
plumbing. Advising keeps an honest failure mode because, on Windows, a bare
handle genuinely has no way to identify which connection point is meant; the
comment makes that reasoning reviewable.

**Production role.** Makes the handle contract functional on the platform the
library targets; determines the actual observable results of default-invoked
capability methods at runtime.

### `idispatch.go` — enumeration overrides on the dispatch wrapper

**What changed.** The dispatch wrapper gains two cursor operations, `Skip`
and `Reset`. Each retrieves the `_NewEnum` collection property through the
wrapper's own `GetProperty`, clears the returned variant, converts it via the
existing accessor on `IUnknown`/`VARIANT` (`ToIUnknown().IEnumVARIANT(...)`),
and then delegates the cursor call with a `Release` deferred. The doc comments
observe that dispatch-surfaced COM collections publish their enumerator
through `_NewEnum`.

**Why this site.** The dispatch wrapper is the one handle whose native
contract includes late binding but not enumeration, while the handle
contract now promises both; code holding only dispatch handles — the most
common user position in `oleutil` call sites — is exactly the population
that would notice capacity gaps. `Next` and `Clone` are *not* overridden
because the dispatch-side fact pattern (walk a collection you only see as a
property) doesn't extend to single-element fetch with array return shapes
naturally, and the inherited defaults were judged sufficient there by the
reviewer — the two cursor controls are the operations with a clean,
self-explanatory composition.

**Why this shape.** Both overrides faithfully use established library
patterns rather than new mechanisms; `newEnum.Clear()` is deferred, the
enumerator is released, and the delegation follows existing per-wrapper
methods verbatim. They are real implementations that function correctly
when called.

**Production role.** The dispatch wrapper becomes a plausible full-capability
handle; code holding only it gains collection-control surface (skip/reset)
that actually delegates to the right underlying object.

### `oleutil/oleutil.go` — helpers re-typed onto the handle

**What changed.** Nine helpers change their first parameter from the
concrete dispatch wrapper to the handle interface. `CallMethod`,
`GetProperty`, `PutProperty` and their `Must*` twins narrow to one-line
delegations to the corresponding interface method, with doc-comment updates
("calls method on a COM object..."). `PutPropertyRef` (and its `Must*`
twin) composes the operation explicitly: resolve the display ID for the name,
then invoke with the same dispatch-by-reference flag the helper always used,
preserving the empty-or-not parameter split as a direct local decision.

**Why this site.** The helper package is the entire concrete consumer
surface — no other call sites in the module hold a handle parameter for
late binding; this is where the opening's user value materializes.

**Why this shape.** The handle interface does not carry the
wrapper-specific one-step convenience that admits a dispatch-mode constant,
because such conveniences exist only on concrete wrappers. Delegating to one
interface method requires a target that exists on every handle; composing
from the two primitive steps keeps the helpers' observable dispatch behavior
exactly as before, including the by-reference property write, which has no
shorter primitive on the umbrella.
`Must*` wrappers only adapt the error path and pass through unchanged.
`ForEach` needed only its parameter re-typed — its body already pulled
`_NewEnum` through one property lookup and cast to the concrete enumerator,
which is exactly the shape the umbrella permits.

**Production role.** The consumer boundary: every helper now indicates its
need as "any handle", their bodies locate the capability through the
single-handle abstraction instead of a single concrete wrapper.

### `oleutil/connection_windows.go` and `oleutil/connection_func.go` — connection helper re-typed

**What changed.** `ConnectObject` changes its first parameter from the
concrete dispatch wrapper to the handle interface, in both platform
companions. No body change: the reflected sink construction, the container
lookup through `QueryInterface`, the connection-point `FindConnectionPoint`
and `Advise` calls are untouched.

**Why this site.** The connection flow is the flagship "I have a handle,
give me events" use case (it connects an object to a sink via the container
and point interfaces it queries for itself); if any helper justified the
"any handle" campaign, it is this one, and leaving it concrete while the
rest changed would be the odd review comment in a real series.

**Why this shape.** The helper only exercises `QueryInterface` and value
inspection on its parameter (the connection point and container interfaces
are dynamic locals), so the umbrella's groups beyond identity are unused —
widening here costs nothing and reads as the whole point of the campaign.

**Production role.** Connection setup becomes callable with handles other
than the dispatch wrapper, completing the helper-layer migration.

## Deliberately out of scope of this story

- `CreateObject` and `GetActiveObject` keep returning the concrete root
  handle; their return types are part of established public usage and
  widening them would not unlock any consumption path, since a caller can
  already pass the concrete handle to every re-typed helper as-is.
- The deprecated fluent `Connection`/`Dispatch` wrappers in `connect.go` were
  left completely alone: frozen surface.
- The `stdDispatch` callback shim, the safearray helpers, and the WinRT
  classes participate in no helper-parameter path and were not touched.

## Behavior and compatibility decisions

- No exported declaration is removed or made more restrictive: the change
  adds methods to a wrapper, replaces an unreferenced interface, and widens
  concrete helper parameters to an interface every wrapper satisfies, so all
  existing call sites remain valid.
- Go's method-resolution rules keep runtime behavior for concrete-wrapper
  callers unchanged: the dynamic wrapper's own depth-0 method shadows any
  inherited default or override of the same name, so existing late-binding
  and enumeration paths keep executing the exact code they did before.
- Helper behaviors that users observe — dispatch-mode constants (method
  call, property get, property put, property put-by-reference), the
  empty-arguments handling around invocation, deferred `Release` of every
  interface query, and the error values surfaced for failures — are
  preserved exactly in the re-typed bodies.
