# Maintenance request

I maintain a deployment that builds several sanitization profiles on top of bluemonday, and lately every change I make around policy declarations touches far more of the library than it should. I'd like the declaration path cleaned up end to end. A while back the library grew a single seam type that everything uses to talk to a policy.

I first ran into this in `cmd/sanitise_html_email/main.go`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
