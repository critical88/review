# Maintenance request

Earlier in this cycle we gave all of our generator scripts one shared "frontend" layer so that runner tooling can drive any of them the same way (the root DOM script plus the canvas, jscript, php, vbscript, webgl and webgpu drivers).

I first ran into this while working around `generate_samples` in `canvas/generator.py`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
