# Maintenance request

A recent review of the client entry point found that it has become the default home for unrelated features. It inherits HTTP request behavior while also constructing a separate transport object, so it is no longer clear which session actually sends a request. The same class now contains shortcuts for reading and updating spreadsheet ranges that build API requests themselves. Together these changes make authentication, transport, and endpoint construction difficult to evolve independently, and have introduced a reverse dependency from a general utility module into the client.

I first ran into this while working around `get_config_dir` in `gspread/auth.py`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
