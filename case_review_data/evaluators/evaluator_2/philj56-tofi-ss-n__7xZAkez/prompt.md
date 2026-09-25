# Maintenance request

I maintain tofi, a Wayland dmenu-like launcher. We support per-element theming: the prompt, the input line and its placeholder, the result rows (with an alternate look for odd-indexed rows), the selected row, and the cursor each accept optional colour, background, padding and corner-radius settings, on top of the window-wide `text-colour` and `background-colour`. Please restore single-point ownership of this decision: one place, wherever it genuinely belongs in our architecture, should decide what an unset theme value inherits including the window-wide defaults and the odd-row and cursor subtleties and the drawing paths and the config loader should simply use the decided values.

I first ran into this in `src/config.c`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
