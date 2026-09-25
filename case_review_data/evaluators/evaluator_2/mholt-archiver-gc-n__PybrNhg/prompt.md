# Maintenance request

We maintain this Go archiving library. A while back a "bring back one-call archiving" feature landed: a configurable value was introduced so that embedding programs could register a private subset of formats and call the common operations without composing half a dozen package-level pieces themselves. It works, and users like it single-folder archiving and extract-to-directory became one-liners, and programs that only want, say, tar and gzip can finally avoid registering every format under the sun. Since then, though, that value has become the place where everything gets added, and it is getting hard to live with.

I first ran into this in `archiver.go`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
