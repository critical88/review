import type { ScanRule, RuleDatapoint } from '../types';

export interface CrossFeatureData {
  definingArea: string;
  consumerAreas: string[];
  spreadCount: number;
}

/** Fragments consumed across area boundaries — a quantified signal of coupling
 * between features. The weight is the number of distinct consuming areas. */
export const crossFeatureFragments: ScanRule<CrossFeatureData> = {
  name: 'cross-feature-fragments',
  description: 'Fragments spread across multiple areas of the codebase.',
  create(context) {
    // fragment id -> areas of the definitions that spread it
    const consumerAreas = new Map<string, Set<string>>();
    const spreadCount = new Map<string, number>();

    const graph = context.getModuleGraph();
    const fragments = context.getFragmentGraph();
    // Resolve the area of every module the graph knows about up front, so
    // spread sites and defining modules can be labelled from one table.
    const moduleAreas = new Map<string, string>();
    for (const [module, imports] of graph.importMap()) {
      if (!moduleAreas.has(module)) moduleAreas.set(module, graph.areaOf(module));
      for (const target of imports) {
        if (!moduleAreas.has(target)) moduleAreas.set(target, graph.areaOf(target));
      }
    }

    return {
      visitor: {
        FragmentSpread: {
          enter(node) {
            const definition = context.getCurrentDefinition();
            if (!definition) return;
            const id = fragments.resolve(definition.schemaName, node.name.value);
            if (!id) return;
            let areas = consumerAreas.get(id);
            if (!areas) consumerAreas.set(id, (areas = new Set()));
            areas.add(moduleAreas.get(definition.module) ?? graph.areaOf(definition.module));
            spreadCount.set(id, (spreadCount.get(id) || 0) + 1);
          },
        },
      },

      collect() {
        const byId = new Map(context.fragments.map((fragment) => [fragment.id, fragment]));
        const datapoints: RuleDatapoint<CrossFeatureData>[] = [];
        for (const [id, areas] of consumerAreas) {
          const fragment = byId.get(id);
          if (!fragment) continue;
          const definingArea = moduleAreas.get(fragment.module) ?? graph.areaOf(fragment.module);
          // Areas other than where the fragment itself lives.
          const externalAreas = [...areas].filter((area) => area !== definingArea).sort();
          if (!externalAreas.length) continue;

          datapoints.push({
            ref: { kind: 'fragment' as const, id },
            message: `Fragment '${fragment.name}' (${definingArea}) is consumed by ${externalAreas.length} other area(s)`,
            weight: externalAreas.length,
            data: {
              definingArea,
              consumerAreas: [...areas].sort(),
              spreadCount: spreadCount.get(id) || 0,
            },
          });
        }
        return datapoints.sort((a, b) => (b.weight || 0) - (a.weight || 0));
      },
    };
  },
};
