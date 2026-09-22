# Account record logic scattered outside the data-access layer

## What we observed

The account (user) record has exactly one home in this codebase: the data-access layer that declares the record's column mapping, creates the table on first run, performs the lookups, and hashes passwords at insert time. That layer is meant to be the only place that knows what a user row means.

That boundary no longer holds. While adding safeguards to the sign-up and protected-session flows, record-level knowledge leaked outward:

- The **sign-up screen** now fetches candidate rows for the address being registered and re-validates the fetched row field by field (comparing the stored address with the candidate address and probing the stored password) before deciding that the account already exists.
- The **credentials authorization path** in the NextAuth provider wiring now pulls the raw row out of the query result and branches on the row's own fields before letting the bcrypt comparison run, instead of asking the data layer whether these credentials match.
- The **protected dashboard view** now derives its greeting by walking the stored account's identity fields (email, numeric id) through a local helper, instead of simply rendering the session's account identity.

In each spot, the code is far more interested in the account record's internals than in the job of the module it lives in. Any change to how an account row is shaped, looked up, or hashed has to be discovered and repeated in screens and auth wiring that should not care.

## What we want

Re-establish the data locality of the account record:

- Put the decisions about account rows back behind the data-access layer so it exposes outcome-level operations (does this address already exist, do these credentials match, which account is this) instead of handing raw rows to its callers.
- The sign-up flow, the credentials authorization path, and the protected dashboard view should each consume one clear, intention-revealing operation rather than dissecting record fields locally.
- Investigate the whole account-lifecycle path (registration, credential sign-in, logged-in rendering) and address every place where presentation or auth wiring currently re-derives record-level rules; do not stop at the first site you fix.
- Give the moved record logic a home that shares the record's own lifetime and queries (the persistence side), and remove whatever local record-inspection scaffolding becomes unnecessary. Do not leave the now-unused helpers behind.

## Behavior that must not change

- Sign-up for an already-registered address still yields the "User already exists" result instead of inserting a second row; a fresh address still creates the account and redirects to the login page.
- Sign-in still succeeds only with the password matching the stored hash, and the matched account still becomes the NextAuth session user.
- The protected page still shows the signed-in account's email and keeps a working sign-out form.
- The NextAuth public surface (GET/POST handlers, `auth`, `signIn`, `signOut`), the `/login` and `/protected` route gating performed by the middleware, and the table bootstrap behavior all remain as they are.

## Notes

- The persistence module must stay the only place that talks to the database; do not add raw queries elsewhere.
- Keep the Edge-compatible auth configuration free of Node-only imports; the middleware environment cannot load the database driver, so route decisions there must not start depending on the persistence module.
