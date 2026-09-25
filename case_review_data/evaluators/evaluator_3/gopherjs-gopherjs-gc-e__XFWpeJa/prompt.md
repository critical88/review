# Maintenance request

I maintain GopherJS, and our build front end has slowly drifted into a shape that is hurting us. I want to untangle it. For a while now, whenever per-package build work needed anything, the path of least resistance has been "the object that drives the build already sees every package, so let it handle this too.

I first ran into this in `build/augment.go`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
