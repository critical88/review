# Finish the storage split: the hook still does the owners' work

You are working in a small React hook library. The public entry is the `useLocalStorageState` hook — it mirrors `useState` on top of `localStorage` and exports the `inMemoryData` map used when browser storage throws. The repository is at `/workspace/repo`.

## What we were doing

We started preparing the storage layer for two things we actually need: interchangeable storage backends (the browser API is not the only target anymore) and a notification mechanism shared by every hook instance for a key. As a first step, the storage state, the per-key bookkeeping, and the instance sync were pulled out of the hook module into helper modules so several pieces could evolve independently.

The split moved the data, but it stopped there. Look at how the pieces talk today:

- the hook's callbacks still compute everything themselves — they read and write the helper objects' fields directly, keep the per-key caches, decide fallback-versus-storage, and drive first-read default persistence;
- one helper object's operation reaches into the fields of another storage helper to finish its job;
- subscription bookkeeping is done by whoever subscribes, by editing the registry's listener collection in place.

The helper objects we extracted ended up as bags of public data: other code does their work for them, from the outside.

## The diagnosis

This is textbook feature envy. The behavior sits in functions that are far more interested in another object's data than in their own, so every change to storage semantics has to be re-encoded among all the coordinating callbacks instead of landing in one place. The data and the operations that keep them consistent have drifted apart.

## What we want

Finish the move so behavior lives with the data it keeps consistent. For each piece of behavior in this storage area, decide which owner that behavior belongs to — the object holding the shared storage area and in-memory fallback pool, the owner of one key's cached value, the owner of the per-key listener bookkeeping, or the hook itself — and put the operations there:

- each helper object should keep its data private behind operations that do its own work, including the error paths (storage reads that throw, seeding and recovering the in-memory fallback, persisting first-read defaults);
- the hook and its callbacks should ask those owners to do things instead of manipulating their fields — for subscription, snapshotting, setting, and removing values alike;
- cross-owner cooperation should happen through owner operations, in whichever direction the data ownership warrants, not by one object rewriting another object's state.

It is fine to restructure, merge, rename, or re-split the helper modules while doing this — the interesting outcome is where the behavior ends up, not the current module boundaries. Keep it a real design pass: forwarding shims that merely re-expose the same data to the same callers do not move anything.

## What must keep working

The behavior contract is guarded by the existing test suite and must not change:

- the hook's public surface: a `[value, setState, { isPersistent, removeItem }]` tuple with `useState`-compatible setter semantics, including functional updates;
- `inMemoryData` stays a module-level importable and clearable `Map` from the hook module, shared across instances, so callers (and our tests) can empty the in-memory pool;
- keys can change between renders and the instance re-targets cleanly;
- first-read default persistence, the default JSON-like serialization (including the `undefined` case), and per-key custom serializers keep working exactly as now;
- writes persist through the resolved storage area and fall back to memory exactly when storage throws, with `isPersistent` reporting the fallback state, and `removeItem` clearing both;
- one instance's write is seen by other instances for the same key, and cross-tab `storage` events re-render matching instances while the `storageSync` option is honored;
- nothing touches `localStorage` eagerly at import time; browser-storage access stays late and guarded so tests can install storage doubles after import;
- server rendering never reaches browser storage.

We care that the suite keeps passing throughout the change, not only at the end.

## Verifying

```sh
npm install
npx tsc
npx vitest --run
```

Run the suite before you start to see the baseline, and keep it green while you move behavior around.
