/**
 * Optional build-time constant.
 *
 * Distributors can re-enable the pre-v3.0 legacy print handling paths by
 * defining this constant through their bundler (for example Webpack's
 * `DefinePlugin`). The published `react-to-print` package never defines it,
 * so the legacy paths are compiled in but permanently disabled.
 */
declare const REACT_TO_PRINT_LEGACY_FLOW: boolean | undefined;
