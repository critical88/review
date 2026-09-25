# Maintenance request

Rally records a small but meaningful picture of the cluster a race benchmarks: its distribution flavor and version, its build revision, the identifier of the benchmark target (cluster or project, depending on what kind of installation it is), the platform it runs on (serverless, hosted, or on-prem), and the authentication style Rally used to connect (API-key or basic auth). The problem is how these facts move through the code.

I first ran into this while working around `BenchmarkComplete` in `esrally/driver/driver.py`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
