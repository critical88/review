# Maintenance request

We maintain aqua's package installation path: the code that downloads release assets, verifies them, extracts them, and installs tools from Go module sources, local Go sources or Rust crates. Lately every change to this area hurts. When we added the last signature verification scheme we expected to touch the family's helper type and its configuration, but the real work turned out to live inside the main installation component — parameter assembly, command execution, and the implementation of every verification family sit there now. The helper types still exist with plausible-looking methods, but following any of those methods leads straight back into the central component, which then reads the helper's data and does the job itself.

I first ran into this while working around `NewCargoPackageInstallerImpl` in `pkg/installpackage/cargo.go`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
