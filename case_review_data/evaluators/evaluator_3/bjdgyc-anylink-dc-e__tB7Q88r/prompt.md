# Maintenance request

I maintain anylink (OpenConnect SSL VPN server in Go). Over the past releases we have been adding client context — where a login came from, what client software it used, what mac address the tunnel reported, what device is on the other end — so that support can answer "who connected from where and why did they get that ip".

I first ran into this while working around `GroupAuthLogin` in `server/dbdata/group.go`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
