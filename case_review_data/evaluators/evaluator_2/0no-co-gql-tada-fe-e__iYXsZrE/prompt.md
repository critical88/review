# Maintenance request

We maintain the `gql.tada scan` command — the analysis pass that reports on a project's GraphQL documents and on how the codebase organises them. A pattern has crept into the rule code and we'd like it dealt with. While preparing a change to how the graph classifies module areas, we found ourselves repeatedly scrolled into rule implementations that contain their own versions of the same graph computations — code that figures out who depends on whom, how far each module sits from the import-graph roots, or which directory area a module belongs to, all derived directly from the graph's import data.

I first ran into this while working around `crossFeatureFragments` in `packages/cli-utils/src/commands/scan/rules/cross-feature-fragments.ts`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
