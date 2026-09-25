import type { ScanRule, RuleDatapoint } from '../types';

export interface FetchDepthData {
  /** Shortest import-distance from an entry point to the defining module. */
  depth: number | null;
  area: string;
  module: string;
}

/** Where each query sits in the module graph: how far its defining module is
 * from an entry point. As a distribution this describes data-fetching placement
 * (hoisted to route boundaries vs. scattered deep in the tree, i.e. waterfalls).
 *
 * Restricted to queries: mutations and subscriptions are naturally triggered
 * deep in the tree (event handlers, forms), so their depth carries no signal. */
export const fetchDepth: ScanRule<FetchDepthData> = {
  name: 'fetch-depth',
  description: 'Distance from an entry point to where each query is defined.',
  create(context) {
    // Reads the static module graph in collect(); no traversal state needed.
    return {
      visitor: {},
      collect() {
        const graph = context.getModuleGraph();
        // Walk the import graph outwards from the entry points, recording how
        // far each module sits, then place each query by that distance.
        const distance = new Map<string, number>();
        let frontier = [...graph.entryPoints()];
        let depth = 0;
        while (frontier.length) {
          const next: string[] = [];
          for (const node of frontier) {
            if (distance.has(node)) continue;
            distance.set(node, depth);
            for (const to of graph.importMap().get(node) || []) {
              if (!distance.has(to)) next.push(to);
            }
          }
          frontier = next;
          depth++;
        }
        // Place each query by the distance recorded above, labelling it with
        // the area of its defining module as we go.
        const datapoints: RuleDatapoint<FetchDepthData>[] = [];
        for (const op of context.operations) {
          if (op.kind !== 'query') continue;
          const entryDistance = distance.get(op.module);
          const moduleArea = graph.areaOf(op.module);
          datapoints.push({
            ref: { kind: 'operation' as const, id: op.id },
            message: `${op.name || '(anonymous)'} is ${
              entryDistance == null
                ? 'unreachable from any entry point'
                : `${entryDistance} hop(s) from an entry point`
            }`,
            weight: entryDistance ?? undefined,
            data: { depth: entryDistance ?? null, area: moduleArea, module: op.module },
          });
        }
        return datapoints.sort((a, b) => (b.data.depth ?? -1) - (a.data.depth ?? -1));
      },
    };
  },
};
