import { bindingScopeValues, bindingTypeValues } from '@inversifyjs/core';

import { ContainerSnapshot } from './container_snapshot';

export class SnapshotStatistics {
  public computeScopeBreakdown(
    snapshot: ContainerSnapshot,
  ): Map<string, number> {
    const breakdown: Map<string, number> = new Map<string, number>();

    for (const digest of snapshot.digests) {
      const lifecycleSuffix: string =
        digest.hasActivation || digest.hasDeactivation ? '+lifecycle' : '';
      const key: string = `${digest.scope}/${digest.bindingType}${lifecycleSuffix}`;
      const count: number = breakdown.get(key) ?? 0;

      breakdown.set(key, count + 1);
    }

    return breakdown;
  }

  public countStatefulRegistrations(snapshot: ContainerSnapshot): number {
    let stateful: number = 0;

    for (const digest of snapshot.digests) {
      const keepsInstance: boolean =
        digest.scope === bindingScopeValues.Singleton ||
        digest.bindingType === bindingTypeValues.ConstantValue;
      const runsLifecycle: boolean =
        digest.hasActivation || digest.hasDeactivation;

      if (keepsInstance || runsLifecycle) {
        stateful += 1;
      }
    }

    return stateful;
  }

  public computeTagUsage(snapshot: ContainerSnapshot): Map<string, number> {
    const usage: Map<string, number> = new Map<string, number>();

    for (const digest of snapshot.digests) {
      for (const tagName of digest.tagNames) {
        const count: number = usage.get(tagName) ?? 0;

        usage.set(tagName, count + 1);
      }

      if (digest.name !== undefined) {
        const qualifierKey: string = `name:${digest.name}`;
        const qualifierCount: number = usage.get(qualifierKey) ?? 0;

        usage.set(qualifierKey, qualifierCount + 1);
      }

      if (digest.tagNames.length === 0 && digest.name === undefined) {
        const unqualifiedKey: string = '(unqualified)';
        const unqualifiedCount: number = usage.get(unqualifiedKey) ?? 0;

        usage.set(unqualifiedKey, unqualifiedCount + 1);
      }
    }

    return usage;
  }
}
