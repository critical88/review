# Maintenance request

We shipped the embedding API for fiche the obvious way: each behavior an operator might want to change (where and how pastes are stored, how status and access are logged, how the process and link identity are set up, how incoming connections are admitted) got its own set of callbacks, and to keep hosting uniform we put all of those callbacks behind one registration object that a host fills before booting.

I first ran into this while working around `fiche_connection` in `fiche.c`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
