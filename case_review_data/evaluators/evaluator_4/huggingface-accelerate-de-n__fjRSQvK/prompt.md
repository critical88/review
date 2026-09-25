# Maintenance request

While preparing the v1.16 release notes we went back over the staged FSDP2 conversion that became the default in v1.15.0. The `fsdp_version=2` layout (with `reshard_after_forward` and the rest of the v1.15 field surface) has now been the default for a full release, and the one-release windows we kept for launchers and configs written against the pre-v1.15 argument surface have expired.

I first ran into this while working around `load_accelerator_state` in `src/accelerate/checkpointing.py`; please start there and follow the related call path.

Please remove the obsolete path and the production scaffolding that exists only to support it, while keeping the active flow straightforward.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
