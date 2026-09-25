# Maintenance request

After the 6.0 transport and authentication migration, a maintenance pass found that the live API follows the new construction model but parts of the old compatibility machinery are still present in production modules. Those remnants are no longer reached by supported callers, yet they obscure which path is authoritative and make routine cleanup look riskier than it is.

I first ran into this while working around `api_key` in `gspread/auth.py`; please start there and follow the related call path.

Please remove the obsolete path and the production scaffolding that exists only to support it, while keeping the active flow straightforward.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
