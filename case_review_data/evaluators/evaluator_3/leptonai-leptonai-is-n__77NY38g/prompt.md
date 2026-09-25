# Maintenance request

While wiring small workspace tools on top of the v2 Python SDK the resource API layer behind the client's `client.deployment`, `client.job`, `client.secret`-style attributes we recently standardized how resource APIs declare what they can do, so that one generic driver could walk every workspace resource uniformly. Living with that change for a few weeks has surfaced a problem I can no longer unsee, and I want it fixed properly rather than patched around. The symptom I hit first: our resource dashboard renders the *same* capability controls for every kind of resource.

I first ran into this in `leptonai/api/v2/dedicated_node_groups.py`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
