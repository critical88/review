# Maintenance request

While fixing table paging, we found that rendering configuration is spread across long positional argument lists. The table renderer passes the same border, sizing, offset, wrapping, and style information through its border, header, row, overflow, and footer helpers. The tree renderer has a similar pattern for inherited style and layout context. The individual values are valid, but they describe a small number of cohesive rendering contexts. Adding an override or changing sizing policy means editing a chain of signatures and manually preserving argument order. It is also difficult to tell which values a helper may safely change for its children.

I first ran into this while working around `resize` in `table/resizing.go`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
