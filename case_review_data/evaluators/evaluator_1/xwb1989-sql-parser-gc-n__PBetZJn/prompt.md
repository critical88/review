# Maintenance request

The buffer used to render parsed statements has become the home for several different operations. Besides template rendering and bind-position tracking, it now constructs executable queries from bind values, produces redacted text for logging, and emits impossible or shadow queries. Each operation has its own error and encoding rules, but they are coupled through one growing object. This makes even a small change to redaction or bind expansion require working through unrelated rendering state. Give bound-query construction, redaction, and impossible-query production cohesive owners while leaving the rendering buffer focused on rendering. Shared encoding or bind lookup should have one implementation rather than being copied between the new owners.

I first ran into this in `impossible_query.go`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
