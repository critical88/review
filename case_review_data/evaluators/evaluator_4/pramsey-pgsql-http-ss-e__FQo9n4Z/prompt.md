# Maintenance request

Context: pgsql-http is a PostgreSQL extension for making HTTP requests from inside the database. The heavy lifting lives in one native C module performing requests on a shared libcurl handle; sessions tune their options (timeouts, proxy, TLS, keepalive) through the extension's option plumbing, which also mirrors them into custom GUCs. Build with PGXS `make` / `make install`; the regression suite runs with `make installcheck` against a local test endpoint. Over the last while we kept getting bug reports of the same family: a session changes something about how it wants its outbound requests to connect, and some of our code paths honor it while others do not.

I first ran into this while working around `_PG_init` in `http.c`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
