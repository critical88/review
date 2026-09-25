# Maintenance request

Maintaining starter workflows currently requires keeping the validation script and the GHES synchronization script in agreement about the same template conventions. Categories, filename rules, reusable-workflow handling, and enterprise eligibility are partly configured in each script and partly registered by helper modules as import-time side effects. The GHES path only sees some rules because it imports validation code that it does not otherwise use. This arrangement makes a simple convention change surprisingly risky: the rule may be updated for CI validation but not enterprise synchronization, or a new helper may need to be imported solely to populate a shared table.

I first ran into this while working around `WorkflowsCheckResult` in `script/sync-ghes/index.ts`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
