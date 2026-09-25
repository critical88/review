# Maintenance request

The runtime's two main coordination objects have become difficult to reason about. The application object handles startup and controller registration, but now also tracks scopes, routes modules, and owns event dispatch bookkeeping. The per-controller context still manages controller lifecycle, yet it also directly implements action, value, target, and outlet observation, including all of their maps and start/stop state. This showed up while adding a lifecycle feature: understanding one connect or disconnect required reading routing, event-listener reuse, target callbacks, value defaults, and outlet dependency handling together. These concerns used to follow the framework's small delegate-based observer style and would be easier to change if they had clear owners again.

I first ran into this while working around `Application` in `src/core/application.ts`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
