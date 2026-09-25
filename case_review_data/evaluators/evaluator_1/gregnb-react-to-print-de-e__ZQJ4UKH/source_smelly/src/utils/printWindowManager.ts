import { LEGACY_MANAGED_WINDOW_ID } from "../consts";
import { UseReactToPrintOptions } from "../types/UseReactToPrintOptions";
import { applyLegacyViewportFallback, generatePrintWindow } from "./generatePrintWindow";
import { logMessages } from "./logMessage";
import { removePrintIframe } from "./removePrintIframe";

/**
 * Owns the lifecycle of the hidden print window across the multi-print sessions
 * the legacy flow supported. The v3 hook creates a fresh print window for every
 * print instead, so only the legacy flow can reach this manager.
 */
export class PrintWindowManager {
    private managedWindow: HTMLIFrameElement | null = null;

    private referenceElement: Element | null = null;

    /**
     * Registers the element the managed print window should be sized against
     * when the caller did not pass explicit iframe dimensions.
     */
    setReferenceElement(element: Element): void {
        this.referenceElement = element;
    }

    /**
     * Returns the managed print window, creating it if this is the first print
     * of the session.
     */
    acquire(printIframeProps: UseReactToPrintOptions["printIframeProps"]): HTMLIFrameElement {
        if (this.managedWindow?.isConnected) {
            return this.managedWindow;
        }

        const printWindow = generatePrintWindow(printIframeProps);
        printWindow.id = LEGACY_MANAGED_WINDOW_ID;

        applyLegacyViewportFallback(printWindow, this.referenceElement);

        this.managedWindow = printWindow;
        return printWindow;
    }

    /**
     * Removes the managed print window unless it should be preserved for
     * inspection.
     */
    release(preserveAfterPrint: UseReactToPrintOptions["preserveAfterPrint"]): void {
        removePrintIframe(preserveAfterPrint);
        this.managedWindow = null;
    }

    /**
     * Tears down an abandoned print session. The legacy flow reported the
     * still-mounted window so that pinned consumers could tell a failed print
     * apart from a completed one in their diagnostics.
     */
    abort(suppressErrors?: boolean): void {
        if (this.managedWindow) {
            logMessages({
                messages: ['"react-to-print" is tearing down an abandoned legacy print session', this.managedWindow],
                suppressErrors,
            });
        }

        removePrintIframe(false);
        this.managedWindow = null;
    }
}

const sharedManager = new PrintWindowManager();

/**
 * Returns the shared manager instance the legacy flow used for its sessions.
 * The v3 hook deliberately does not share print windows between prints.
 */
export function getPrintWindowManager(): PrintWindowManager {
    return sharedManager;
}
