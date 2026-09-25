import { logMessages } from "./logMessage";

/** Size measurements for the hidden print window used by the print preview spike. */
export interface PrintViewport {
    /** CSS pixel width, e.g. "800px" */
    width: string;
    /** CSS pixel height, e.g. "600px" */
    height: string;
}

/** The smallest size the preview rendered before applying measured content sizes. */
export const MINIMUM_PREVIEW_VIEWPORT: PrintViewport = {
    width: "480px",
    height: "640px",
};

/**
 * Reads a CSS pixel length such as `"800px"` and returns the numeric part, or
 * `null` when the value is not a plain pixel length.
 */
function readPixelLength(cssLength: string): number | null {
    if (!cssLength.endsWith("px")) {
        return null;
    }

    const parsed = Number.parseInt(cssLength.slice(0, -2), 10);
    return Number.isNaN(parsed) ? null : parsed;
}

/**
 * Clamps the measured content box of a reference element to the minimum preview
 * size. An early draft of the iframe sizing feature (`printIframeProps`
 * `width`/`height`) used this resolver; the shipped feature applies the declared
 * props directly instead.
 */
export function resolvePrintViewport(referenceElement?: Element | null): PrintViewport {
    if (!referenceElement) {
        return { ...MINIMUM_PREVIEW_VIEWPORT };
    }

    const boundingRect = referenceElement.getBoundingClientRect();
    const minimumWidth = readPixelLength(MINIMUM_PREVIEW_VIEWPORT.width) ?? 480;
    const minimumHeight = readPixelLength(MINIMUM_PREVIEW_VIEWPORT.height) ?? 640;

    const viewport: PrintViewport = {
        width: `${Math.round(Math.max(boundingRect.width, minimumWidth))}px`,
        height: `${Math.round(Math.max(boundingRect.height, minimumHeight))}px`,
    };

    logMessages({
        level: "debug",
        messages: ['"react-to-print" resolved a preview viewport of', viewport],
    });

    return viewport;
}
