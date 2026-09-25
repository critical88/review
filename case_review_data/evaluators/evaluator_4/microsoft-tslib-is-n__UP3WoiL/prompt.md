# Maintenance request

Our compile targets' emitted helpers live in a small runtime that ships in interchangeable copies for every consumption mode classic script tags, CommonJS, and native ESM plus the declaration file downstream tooling reads. A consumer of the downlevel generator support filed a bug saying the generator objects we return "have a bunch of extra function properties" they diff our returned objects' own property names against what the native runtime produces, and ours no longer matches what the language gives them for the same feature. They only noticed because the properties do not show up in `Object.keys`, which itself is inconsistent and smells like the install path was trying to have it both ways.

I first ran into this in `tslib.d.ts`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
