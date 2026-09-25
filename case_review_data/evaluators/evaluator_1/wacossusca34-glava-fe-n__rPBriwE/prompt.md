# Maintenance request

I just finished wiring up "audio sessions that follow the renderer's requests" and before this hardens into the tree I want a second pair of eyes on the module split, because I think I talked myself into a bad arrangement while following my own convenience. Some history for context.

I first ran into this while working around `audio_data` in `glava/fifo.c`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
