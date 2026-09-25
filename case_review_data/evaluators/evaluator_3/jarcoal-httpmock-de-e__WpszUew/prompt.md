# Maintenance request

httpmock answers HTTP requests from the mock responders you register. Its route resolution is tolerant on purpose: a request is looked up by its full registered URL first, then with sorted query parameters, then with the query string stripped, then by path alone, and activation of mocking as a whole goes through the `GONOMOCKS` environment variable.

I first ran into this in `env.go`; please start there and follow the related call path.

Please remove the obsolete path and the production scaffolding that exists only to support it, while keeping the active flow straightforward.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
