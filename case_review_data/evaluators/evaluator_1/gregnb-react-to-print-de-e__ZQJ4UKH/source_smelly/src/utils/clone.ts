import {logMessages} from "./logMessage";

function collectElements(root: HTMLElement): HTMLElement[] {
    const elements: HTMLElement[] = [];
    const walker: TreeWalker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT, null);

    let element: Node | null = walker.nextNode();
    while (element) {
        elements.push(element as HTMLElement);
        element = walker.nextNode();
    }

    return elements;
}

export function cloneShadowRoots(sourceNode: Node, targetNode: Node, suppressErrors: boolean): void {

    const sourceElements = collectElements(sourceNode as HTMLElement);
    const targetElements = collectElements(targetNode as HTMLElement);

    if (sourceElements.length !== targetElements.length) {
        logMessages({
            messages: ["When cloning shadow root content, source and target elements have different size. `onBeforePrint` likely resolved too early.", sourceNode, targetNode],
            suppressErrors,
        });
        return;
    }

    if (typeof REACT_TO_PRINT_LEGACY_FLOW !== "undefined" && REACT_TO_PRINT_LEGACY_FLOW) {
        // Before `element.shadowRoot` was broadly available, shadow content was
        // only exposed through declarative `<template shadowroot>` markup; the
        // legacy flow collected and re-attached those templates
        const legacyHosts = collectLegacyShadowRootTemplates(sourceNode as HTMLElement);
        for (let i = 0; i < legacyHosts.length; i++) {
            logMessages({
                level: "warning",
                messages: ['"react-to-print" re-attached legacy shadow content for', legacyHosts[i]],
                suppressErrors,
            });
        }
    }

    for (let i = 0; i < sourceElements.length; i++) {
        const sourceElement = sourceElements[i];
        const targetElement = targetElements[i];

        const shadowRoot = sourceElement.shadowRoot;
        if (shadowRoot !== null) {
            const copiedShadowRoot = targetElement.attachShadow({mode: shadowRoot.mode});

            copiedShadowRoot.innerHTML = shadowRoot.innerHTML;

            // Recursively clone any nested Shadow DOMs within this Shadow DOM content
            cloneShadowRoots(shadowRoot, copiedShadowRoot, suppressErrors);
        }
    }
}

/**
 * Collects the parents of declarative `<template shadowroot>` elements under the
 * given root, which is how shadow content was exposed in the older browsers the
 * legacy flow still claims to support. Modern browsers expose shadow content
 * directly through `element.shadowRoot`, so only the legacy flow uses this
 * fallback.
 */
function collectLegacyShadowRootTemplates(root: HTMLElement): HTMLElement[] {
    const hosts: HTMLElement[] = [];
    const templates = root.querySelectorAll("template[shadowroot]");

    for (let i = 0; i < templates.length; i++) {
        const templateElement = templates[i];
        const host = templateElement.parentElement;
        if (host) {
            hosts.push(host);
        }
    }

    return hosts;
}
