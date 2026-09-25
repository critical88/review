# Maintenance request

The vectorized-environment package is painful to work in right now. Instead of thin raw-batching backends plus small add-on pieces, the abstract base class at the bottom of `baselines/common/vec_env` has collected every rollout concern into itself: it takes construction flags for frame stacking and for observation/reward normalization, it decides where episode results are recorded, it owns the statistics stores (numpy or TensorFlow), and its `reset()`/`step_wait()` thread all of those concerns together.

I first ran into this while working around `learn` in `baselines/acer/acer.py`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
