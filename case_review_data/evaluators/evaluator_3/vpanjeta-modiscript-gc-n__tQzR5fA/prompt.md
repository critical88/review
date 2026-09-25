# Maintenance request

The language front end has gradually absorbed work that belongs to other parts of the runtime. In addition to turning source text into tokens, it now keeps a private word-correction table, validates the required program opening and closing words, and decides when and where command-line debug reports are written. As a result, changing token grammar requires understanding file handling, program packaging, correction policy, and CLI diagnostics at the same time. Restore clear ownership across the pipeline.

I first ran into this while working around `ModiScript` in `modiscript/api.py`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
