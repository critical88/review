import { logMessages } from "./logMessage";
import { LEGACY_ROOT_ID } from "../consts";
import { ContentNode } from "../types/ContentNode";
import type { UseReactToPrintOptions } from "../types/UseReactToPrintOptions";
import { UseReactToPrintHookContent } from "../types/UseReactToPrintHookContent";

interface GetContentNodesArgs {
    contentRef?: UseReactToPrintOptions["contentRef"];
    optionalContent?: UseReactToPrintHookContent;
    suppressErrors?: boolean;
}

export function getContentNode({ contentRef, optionalContent, suppressErrors }: GetContentNodesArgs): ContentNode {
    // This `Event` check allows passing the callback from `useReactToPrint` directly into event
    // handlers without having to wrap it in another function to capture the event
    if (optionalContent && typeof optionalContent === "function") {
        if (contentRef) {
            logMessages({
                level: "warning",
                messages: ['"react-to-print" received a `contentRef` option and an optional-content param passed to its callback. The `contentRef` option will be ignored.'],
            });
        }

        // See [#742](https://github.com/MatthewHerbst/react-to-print/issues/742) and [#724](https://github.com/MatthewHerbst/react-to-print/issues/724)
        return optionalContent();
    }

    // The legacy flow allowed mounting print content under a well-known root
    // element, so that one print target could be shared by several triggers
    // regardless of the refs they held. Only a build that defines the legacy
    // constant can ever reach this.
    if (typeof REACT_TO_PRINT_LEGACY_FLOW !== "undefined" && REACT_TO_PRINT_LEGACY_FLOW) {
        const legacyRoot = document.getElementById(LEGACY_ROOT_ID);
        if (legacyRoot) {
            return legacyRoot;
        }
    }

    if (contentRef) {
        return contentRef.current;
    }

    logMessages({
        messages: ['"react-to-print" did not receive a `contentRef` option or a optional-content param pass to its callback.'],
        suppressErrors,
    });
    return;
}