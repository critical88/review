import { BindingDigest } from './binding_digest';
import { ContainerSnapshot } from './container_snapshot';

export interface SnapshotDifference {
  readonly kind: 'added' | 'qualification' | 'registration';
  readonly serviceIdentifier: string;
  readonly description: string;
}

export class SnapshotComparator {
  public diff(
    baseline: ContainerSnapshot,
    current: ContainerSnapshot,
  ): SnapshotDifference[] {
    const baselineByIdentity: Map<string, BindingDigest> = new Map<
      string,
      BindingDigest
    >();

    for (const baselineDigest of baseline.digests) {
      baselineByIdentity.set(baselineDigest.serviceIdentifier, baselineDigest);
    }

    const differences: SnapshotDifference[] = [];

    for (const digest of current.digests) {
      const previousDigest: BindingDigest | undefined = baselineByIdentity.get(
        digest.serviceIdentifier,
      );

      if (previousDigest === undefined) {
        differences.push({
          description: `registered after baseline (scope=${digest.scope}, ${digest.bindingType})`,
          kind: 'added',
          serviceIdentifier: digest.serviceIdentifier,
        });

        continue;
      }

      const changeDescription: string | undefined =
        this.describeRegistrationChange(previousDigest, digest);

      if (changeDescription !== undefined) {
        differences.push({
          description: changeDescription,
          kind: 'registration',
          serviceIdentifier: digest.serviceIdentifier,
        });
      }
    }

    return differences;
  }

  public describeRegistrationChange(
    previousDigest: BindingDigest,
    updatedDigest: BindingDigest,
  ): string | undefined {
    if (previousDigest.scope !== updatedDigest.scope) {
      return `scope: ${previousDigest.scope} -> ${updatedDigest.scope}`;
    }

    if (previousDigest.bindingType !== updatedDigest.bindingType) {
      return `binding type: ${previousDigest.bindingType} -> ${updatedDigest.bindingType}`;
    }

    if (previousDigest.name !== updatedDigest.name) {
      return `name: ${previousDigest.name ?? '(none)'} -> ${updatedDigest.name ?? '(none)'}`;
    }

    if (previousDigest.tagNames.length !== updatedDigest.tagNames.length) {
      return `tags: ${previousDigest.tagNames.length.toString()} -> ${updatedDigest.tagNames.length.toString()}`;
    }

    if (previousDigest.hasActivation !== updatedDigest.hasActivation) {
      return `activation: ${previousDigest.hasActivation.toString()} -> ${updatedDigest.hasActivation.toString()}`;
    }

    return undefined;
  }
}
