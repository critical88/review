# Maintenance request

Some background: users of this framework ship it as a black box, and when a report comes in ("the app showed a black screen and then Android said it stopped"), we have nothing to reconstruct what their activity actually did. So over time a small support feature grew here: every activity keeps a short record of the steps it went through launch vs. restore from saved state, window gained or lost, focus changes, input-queue attach/detach, permission rounds for a USB device, WebView creation and message hops. The record is a handful of diagnostic fields carried on `struct android_app` (the audit-prefixed ones near the end of the struct), so it is naturally per-activity.

I first ran into this while working around `debug_capture_thread_fn` in `android_native_app_glue.c`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
