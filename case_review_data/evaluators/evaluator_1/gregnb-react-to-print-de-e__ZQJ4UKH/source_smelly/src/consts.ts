export const DEFAULT_PRINT_WINDOW_ID = "printWindow";

/**
 * `id` of the root element the legacy flow mounted its print content under.
 * The v3 hook resolves content through `contentRef` or the callback's
 * optional-content param instead.
 */
export const LEGACY_ROOT_ID = "react-to-print-legacy-root";

/**
 * `id` given to the print window owned by the legacy flow's session manager.
 * The v3 hook creates a fresh, anonymous print window for every print.
 */
export const LEGACY_MANAGED_WINDOW_ID = "react-to-print-legacy-managed-window";
