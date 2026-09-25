# Maintenance request

We have been carrying a growing irritation in the client's pointer-input area, and it recently caused double maintenance on two separate features, so we want it cleaned up properly. The client renders the device screen inside a desktop window. Somewhere in the display code lives everything that defines what the window currently shows — the content rectangle inside the window, the current rotation/flip orientation, the device frame size, and the window handle. Pointer events (mouse and touch) arrive with window coordinates and must be translated into device coordinates before injection.

I first ran into this while working around `sc_input_manager` in `app/src/input_manager.c`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
