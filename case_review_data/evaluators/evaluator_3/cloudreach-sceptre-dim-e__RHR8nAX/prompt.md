# Maintenance request

After a pass over configuration loading and another over Stack construction, there are two methods I keep getting lost in. Each one used to delegate to a few small helpers that each handled one job; someone folded the bodies of those helpers straight into the method and deleted the helpers, so the method now carries the whole job inline and the responsibilities are no longer separated. The configuration read path — the entry point that reads and inherits a StackGroup file — is one of them. The Stack constructor is the other.

I first ran into this while working around `ConfigReader` in `sceptre/config/reader.py`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
