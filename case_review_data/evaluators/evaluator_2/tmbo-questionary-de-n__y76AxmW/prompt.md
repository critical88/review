# Maintenance request

I spent last week onboarding onto the question pipeline and, honestly, a worrying share of my reading time went into code that never runs. The package grew up through the prompt_toolkit 2 days and the old 1.x behaviour, and the cleanup from that era was clearly started but never finished: several places in the pipeline still branch on a module-level constant that is hard-coded to a falsy value and that nothing anywhere writes again.

I first ran into this in `questionary/constants.py`; please start there and follow the related call path.

Please remove the obsolete path and the production scaffolding that exists only to support it, while keeping the active flow straightforward.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
