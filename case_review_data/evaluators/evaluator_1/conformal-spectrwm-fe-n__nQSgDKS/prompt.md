# Maintenance request

Subject: window behavior has drifted into the phase drivers - please pull it back Repository: spectrwm (a keyboard-driven tiling window manager). Practically everything lives in `spectrwm.c`, with the build under `linux/` (`cd linux && make`, which must stay green and warning-free the way it is now). It helped at the time. Then it stayed in the tree. The consequence is that the drivers now know far too much about window internals. I no longer trust that the two copies agree, and I want the window's state machine owned in one place again.

I first ran into this in `spectrwm.c`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
