# Maintenance request

The diff and report commands still work, but both have become difficult to modify. Each command now performs its complete pipeline inside one long function: resolving inputs, loading cached metrics, running comparisons, preparing rows, and publishing the result. The report path also performs its Rich-to-HTML conversion inline, while both commands contain presentation logic that overlaps with the table renderer used elsewhere in the project. This became noticeable while changing output formatting: a small rendering change required understanding metric lookup and comparison state, and the two command paths did not follow the same shared styling rules.

I first ran into this while working around `FileDiff` in `src/wily/commands/diff.py`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
