import { join } from "path";
import { registerTemplateRule } from "./templateRules";

// Starter templates keep their metadata at properties/<id>.properties.json
function propertiesPathFor(folder: string, templateId: string): string {
  return join(folder, "properties", `${templateId}.properties.json`);
}

// Starter workflow files always live next to their properties folder
function workflowPathFor(folder: string, templateId: string): string {
  return join(folder, `${templateId}.yml`);
}

registerTemplateRule("template.properties.join", propertiesPathFor);
registerTemplateRule("template.workflow.join", workflowPathFor);
