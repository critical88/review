# Maintenance request

The client still carries two facilities from an abandoned premium-data beta. One performs a client-side entitlement check before selected paid requests; the other can trim large time-series responses to a preview window. Both were built as optional stages with settings at client, request, and endpoint levels, but the shipped configuration never enables them and the provider rollout has been cancelled. These stages now complicate synchronous and coroutine request paths, endpoint declarations, output formatting, and configuration even though no supported call can observe their behavior.

I first ran into this while working around `AlphaVantage` in `alpha_vantage/alphavantage.py`; please start there and follow the related call path.

Please remove the obsolete path and the production scaffolding that exists only to support it, while keeping the active flow straightforward.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
