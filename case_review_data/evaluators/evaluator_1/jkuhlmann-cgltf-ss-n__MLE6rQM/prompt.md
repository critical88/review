# Maintenance request

I maintain cgltf, and last month I wanted to make a small-sounding change to how object references are handled: tighten what happens when a reference points past the end of an array. I expected to find the rule in one place. I did not. The library resolves a reference a texture's image, a primitive's accessor, a node's mesh, an animation channel's target into a pointer into one of the arrays the parsed model owns.

I first ran into this while working around `cgltf_accessor` in `cgltf.h`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
