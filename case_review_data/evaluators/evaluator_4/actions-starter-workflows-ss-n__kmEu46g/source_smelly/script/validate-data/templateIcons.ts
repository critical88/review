import { join } from "path";
import { registerTemplateRule } from "./templateRules";

// Embedded icon references use the `octicon <name>` form
function iconRefIsOcticon(iconName: string): boolean {
  return /^octicon\s+/.test(iconName);
}

// Locally stored icons live in the shared icons folder
function svgIconPath(iconName: string): string {
  return join("../../icons", `${iconName}.svg`);
}

// GHES sync treats any `octicon`-prefixed reference as an embedded icon
function ghesIconIsEmbedded(iconName: string): boolean {
  return iconName.startsWith("octicon");
}

// The embedded octicon name is everything after `octicon `
function octiconNameFromRef(iconName: string): string | null {
  const match = iconName.match(/^octicon\s+(.*)/);
  return match ? match[1] : null;
}

registerTemplateRule("icons.ref.octicon-prefix", iconRefIsOcticon);
registerTemplateRule("icons.path.repo-svg", svgIconPath);
registerTemplateRule("icons.ghes.kind", ghesIconIsEmbedded);
registerTemplateRule("icons.ref.token", octiconNameFromRef);
