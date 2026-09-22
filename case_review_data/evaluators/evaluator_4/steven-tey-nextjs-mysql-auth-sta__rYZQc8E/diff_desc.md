# Injection design record — account-record behavior moved outside its owner

## Maintenance motivation

This Starter is a deliberately thin email+password auth app. The account record (`User` table: `id`, `email`, `password`) has one natural home: the data-access module that declares the table mapping, creates the table on first run, runs the lookups, and hashes passwords at insert time. Everything else — the NextAuth wiring, the register page's server action, the login form, the protected dashboard — is supposed to consume that module's slim API (`getUser`, `createUser`) and never care how a row is laid out.

Real code bases drift across that boundary for exactly one reason: the person extending a flow already has a row in hand and a small question to answer, so they answer it where they stand. Each individual edit is defensible — "I'm just double-checking the account before I report a duplicate" or "I'm falling back to the id when the email is missing" — because every one of them works. The cost is not visible until the next change to the record's shape or rules, when the maintainer has to go find every place that peeked.

The change in this diff models that drift. It grew out of a plausible maintenance episode: after a complaint about confusing duplicate sign-ups and blank greetings, a developer hardened the three account-facing flows with local safeguards. The safeguards each read like reasonable defensive code; none of them introduces a new capability, and they leave every user-visible flow behaving as before, which is exactly what makes this kind of leak survive review.

## Normal development evolution being modeled

1. **Sign-up hardening.** A report comes in that a double-submitted form created odd feedback. The developer fetches the candidate row (the page already did a `getUser(email)` lookup for the duplicate test) and, while holding that row, decides to verify the record itself before reporting "User already exists": the row must really be for this address and really have a stored password. The check is kept in the page in a small helper next to the form code so the JSX stays readable.
2. **Credential path hardening.** While investigating the same report, the developer notices the authorize callback compares the submitted password against whatever is stored, even for rows with odd data. A guard is added: unpack the row once, confirm the record carries a password and an address, and only then compare. The guard lives inside the provider callback where the flow was already written.
3. **Dashboard identity hardening.** A separate ticket about a blank "You are logged in as …" line leads to a local helper on the protected page that derives the greeting from the account's identity fields step by step (email on file, then the numeric id, then a fallback to the raw session chain), rather than printing a raw session value that might be empty.

Each step is a small, one-file change a maintainer would plausibly make while chasing a symptom, and each leaves the record-level knowledge stranded in flow code.

## Overall design

The three sites were chosen because they are the only places in this codebase where an account row is actually *in hand* at runtime — the sign-up fetched-row guard, the credential-verification row gate, and the session identity render, spanning three distinct lifecycle phases (registration, authentication, logged-in display) and three distinct duties (guarding, verifying, presenting). The record-level work is pushed *outward* from the data module in three different shapes so the same underlying pattern shows up as:

- a **validation-style** dissection (re-checking fetched rows field by field),
- a **branching-style** dissection (gating the stored-hash comparison on the record's own fields),
- a **derivation-style** dissection (walking the record's identity fields to produce a display string).

Deliberate structural variation: one site is a module-level helper next to the flow that uses it, one is inlined into the existing provider callback, and one is a helper colocated with the render it feeds. The intent is that the manifestations cannot all be neutralized by one mechanical pattern-match: each one holds record-level rules of a different kind (uniqueness semantics, stored-credential matching, identity formatting) and each needs a separate decision about what question the flow should really be asking.

## Per-site rationale

### Sign-up duplicate guard — `app/register/page.tsx`

**What changed.** The server action's inline duplication test (`user.length > 0`) now defers to a new module-level helper, `isExistingAccount(user, email)`, right below the page component. The helper reads the first fetched row, then asserts that the row is genuinely for this address (`existing.email === email`) and genuinely carries a stored password before it.

**Why here and why this form.** The register page is the only flow that performs an existence decision with the row in hand, so it is the natural first victim when someone hardens sign-up. A helper (rather than inline conditions in the action) mirrors how such safeguards are actually written: pulled out of the action "for readability" while keeping the row handling right next to the JSX it protects. The fields compared are exactly the record's own (`email`, `password`), so the duplication rule ends up residing in presentation code.

**Production role.** Guards the sign-up flow against duplicate accounts and reports "User already exists"; the account-existence rule of the app now lives on the registration screen.

### Stored-credential gate — `app/auth.ts`

**What changed.** The Credentials `authorize` callback now unpacks the fetched row into `record = user[0]` and gates the bcrypt comparison on the record's own completeness (`record.password && record.email`) before it, returning the record on match and `null` otherwise. The single lookup and comparison of the original flow are otherwise untouched.

**Why here and why this form.** The authorize callback is where the stored record and the submitted credential meet, so a "verify the record before using it" instinct lands here. Inlining the gate into the existing callback (no new helper) matches how defensive tweaks are added to provider wiring, and the row gate is a stronger expression of the same drift than the original single comparison.

**Production role.** Authenticates email+password sign-in; the rule for when stored credentials may be compared and trusted now lives in the auth provider wiring alongside the NextAuth assembly itself.

### Session identity derivation — `app/protected/page.tsx`

**What changed.** The greeting expression `{session?.user?.email}` is replaced by a call to a new module-level helper, `greetingFor(session)`, which walks the stored account's identity: the email on file (`user.email`), failing over to the numeric account id (`user.id`), with a final fallback to the raw session chain plus a generic guest label.

**Why here and why this form.** The protected page renders the signed-in account's identity, and a hardening pass to avoid an empty greeting reaches directly into the account's identity fields. Because rendering needs a display string, this manifestation takes a *derivation* form rather than a guard form — the third distinct shape for the same drift. The final fallback leg also deliberately keeps a raw session field reference in the picture, the way layered fallback code compounds in real projects.

**Production role.** Produces the identity shown on the protected dashboard; the rule for how an account is named is now decided in the view module.

## Ownership background for the unchanged data module

The data-access module was left untouched by this change — that is the point of the drift being modeled: nothing about the record, its table, its hashing rounds, or its queries changed. The leak is purely on the consuming side, which is why it is easy to miss: every individual access is to an exported record the data module already handed out, and every flow still works because the data always satisfies the added checks.
