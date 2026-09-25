# Maintenance request

We drive all of the example firmware in this repository — the flashable projects under `examples/` — through one bring-up flow. The flow flashes an image, halts it, reads a little self-report out of RAM over the debug probe, and steps it to the idle loop. We bumped the bundle protocol revision and nudged the banner layout last sprint, and it took a whole afternoon, three re-opened example projects, and one board that only got fixed when someone noticed its stale banner on the bench.

I first ran into this while working around `initGPIO` in `examples/blinky-ll/blinky.c`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
