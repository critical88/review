# Maintenance request

kiwi-intl is the runtime internationalization engine behind the kiwi toolchain: `IntlFormat.init` builds an i18n instance over the per-language message tables and hands back a reactive proxy that applications call (`get`, `template`, `setLang`, date formatting, ...). Restructure this layer so that **each piece of behavior lives with the object whose data it works on**: the data's owner should compute answers about itself, and its callers should ask instead of digging. Please survey the whole 1.3 layer (`kiwi-intl/src/`), not only the examples above the same authoring shortcut was used everywhere the new data flows and re-home what you find, keeping the collaborators' responsibilities coherent as a whole.

I first ran into this in `kiwi-intl/package.json`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
