# Maintenance request

I spend a lot of time in the `device` package of this WireGuard implementation — mostly in the configuration parser and the handshake receive path — and there is a piece of cleanup I keep postponing that has finally earned its turn. Back when the long routines in those paths were split up into smaller steps, the state each routine had been carrying in plain local variables had to keep flowing into the new steps somehow. The quickest way at the time was to declare that state in the caller and pass each value to every step that needed it.

I first ran into this while working around `CreateMessageInitiation` in `device/noise-protocol.go`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
