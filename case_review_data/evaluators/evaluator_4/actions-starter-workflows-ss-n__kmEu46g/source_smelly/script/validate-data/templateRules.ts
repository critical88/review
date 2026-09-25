// Shared registry for starter template convention rules.
//
// The template conventions (where a starter template keeps its metadata, how
// icon references resolve, and how templates are classified) are contributed as
// named rules by the script that owns them. Consumers select the concrete rule
// serving each convention slot through the "templateRules" map in their
// settings.json, so scripts must agree on both the slot keys and the
// registered rule names.

export interface TemplateRule {
  (...args: any[]): any;
}

const templateRuleTable: { [name: string]: TemplateRule } = {};

export function registerTemplateRule(name: string, rule: TemplateRule): void {
  templateRuleTable[name] = rule;
}

export function templateRuleFor(resolvedName: string): TemplateRule {
  const rule = templateRuleTable[resolvedName];
  if (!rule) {
    throw new Error(`No template rule registered for "${resolvedName}"`);
  }
  return rule;
}
