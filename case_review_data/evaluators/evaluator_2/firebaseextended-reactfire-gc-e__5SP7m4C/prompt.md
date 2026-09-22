# Consolidate ReactFire's runtime state back into cohesive owners

## Context

ReactFire is a set of React hooks over Firebase services. The hooks are deliberately side-effect free, so the little cross-component runtime state the library keeps sits in module-scoped caches that are mirrored onto `globalThis` (so that separate module instances and repeated imports share one copy), and each module keeps its small private helpers next to the hooks that use them. Every data hook funnels through one place: a per-observable caching subject (an `Observable` wrapper that powers suspense, `firstValuePromise`, `initialData` overlay, and re-emission of completed/error state) resolved through a shared observable-resolution hook.

## Problem

In the current tree, that caching-subject class in the observables layer has also become the aggregation point for every shared piece of runtime state and every cross-module helper in the library:

- the global preloaded-observable registry (the cache that `preload*`-style warm-up entry points write into and the observable hooks read from), together with the suspended-request timeout policy that governs it;
- the library-wide preload entry points for the auth user, for Firestore documents, and for generic observables;
- query-identity resolution for Realtime Database lists (`isEqual`-based cache-key assignment);
- query-identity resolution for Firestore collections (`queryEqual`-based cache-key assignment) and the doc-path identity helper for Firestore documents;
- the shared `ReactFireOptions` field checks (valid-key validation with its error branch, `initialData`/`startWithValue` resolution, and `idField` extraction);
- the auth custom-claims validator used by the sign-in check (the factory that produces the `hasRequiredClaims`/`errors` result, including its missing-claim error entries).

These concerns change for different reasons, belong to their service modules or to the shared options surface, and share no state with the class's per-observable subject behavior — yet they are all implemented as class-level members of the subject class, and the other modules reach into it for them. Auth tickets, database tickets, and options-API tickets all first have to edit the observables layer.

## Task

Investigate the observable-caching layer and every hook or helper that reaches into it. Restore cohesive ownership so that each concern lives with the code that changes for the same reason:

- keep the caching subject about one observable again — its value/status/timeout/subscription behavior — not about Firebase services, options validation, or auth;
- generic observable caching and the preload registry belong to the observables layer;
- the preload entry points for the auth user and for Firestore documents should be implemented by their service modules, composed with the shared registry rather than defined by it;
- each service's query-identity resolution belongs with its service code;
- the options field checks belong with the shared options surface the library already publishes them from;
- the custom-claims validator belongs in the auth code that consumes it.

This description names the responsibility areas, not an exhaustive list of affected code points. The full scope is what you discover under `src/` while tracing those responsibilities — fix every place this concentration owns or answers for a concern that is not its own, including leftover pass-through seams that keep public names working but leave implementation in the wrong place, and hooks that ask the caching layer for service-specific identity.

## Behavior and API that must stay stable

This is a pure ownership/refactor change on top of the current tree. The observable behavior must be identical.

- The public API surface is unchanged: every exported hook, option-check helper, and preload entry point keeps its name, parameter signature, export location, and semantics. Code that imports from the package index must continue to work without edits.
- The shared caches keep exactly one instance per `globalThis` key across the whole library. A preloaded observable must remain the same subject the corresponding hook later resolves and reuses (including the auth-user and Firestore-document warm-up paths), preserving this identity in concurrent/server-side usage where modules can be instantiated more than once.
- Observable-id assignment stays collision-free and stable across renders and across equivalent query/reference objects, including `idField`-stamped variants; existing id formats must not change.
- The suspense contract of the observable hooks is unchanged: loading/error/data statuses, `firstValuePromise`, `initialData` overlay behavior, error rethrow, and suspended-request timeout reset.
- Options handling keeps its semantics, including the error raised for invalid option keys, the `initialData`/`startWithValue` resolution precedence, and idField behavior.
- Sign-in claim checking keeps its observable identity per options configuration and its result shape, including the missing-claim error entries produced for required claims.
- `npx tsc` (strict, unused-locals) must pass, and the repository's full test suite (the vitest suites run against the Firebase emulators) must pass unchanged.
