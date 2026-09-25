# Maintenance request

I maintain SmartProxy, a browser extension that keeps proxy subscriptions: server-list subscriptions and rule-list subscriptions that download remote sources on a schedule, plus matching save/test actions on the settings page. A while ago we extracted the duplicated download step (special-request registration, the optional Basic-auth header, and the fetch itself) into one shared helper, because every fix to one copy kept missing the other copy. That was the right call but the way we wired it up is coming back to bite us, and it's now spread across the whole subscription area. Since that change, every place that reads a subscription takes the subscription's connection settings

I first ran into this while working around `SubscriptionUpdater` in `src/core/SubscriptionUpdater.ts`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
