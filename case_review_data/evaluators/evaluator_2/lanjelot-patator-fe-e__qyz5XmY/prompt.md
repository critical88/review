# Maintenance request

We ship a multi-purpose brute-forcing framework whose engine module runs producers, consumers and a log-service process, hands response objects around, and keeps a small record of counters for each worker thread. Somewhere in the last couple of releases the responsibilities drifted: the classes that hold the run's data stopped doing anything with it, and the code that *uses* that data started reaching into their fields directly. The problem sits in the run engine and its reporting pipeline inside the framework's single production module. We would like the data-holding classes (the response results and the per-worker record) to go back to being responsible for their own data: consumers of those objects should ask, not grab.

I first ran into this while working around `MsgFilter` in `src/patator/patator.py`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
