import type { BindingScope, BindingType } from '@inversifyjs/core';
import { bindingScopeValues } from '@inversifyjs/core';

import { BindingDigest } from './binding_digest';
import { ContainerSnapshot } from './container_snapshot';

export interface DigestPayload {
  readonly serviceIdentifier: string;
  readonly scope: BindingScope;
  readonly bindingType: BindingType;
  readonly hasActivation: boolean;
  readonly hasDeactivation: boolean;
  readonly name: string | undefined;
  readonly tagNames: readonly string[];
}

export interface SnapshotPayload {
  readonly containerLabel: string;
  readonly defaultScope: BindingScope;
  readonly digests: readonly DigestPayload[];
}

export class SnapshotCodec {
  public serialize(snapshot: ContainerSnapshot): string {
    const payload: SnapshotPayload = {
      containerLabel: snapshot.containerLabel,
      defaultScope: snapshot.defaultScope,
      digests: snapshot.digests.map((digest: BindingDigest) => ({
        bindingType: digest.bindingType,
        hasActivation: digest.hasActivation,
        hasDeactivation: digest.hasDeactivation,
        name: digest.name,
        scope: digest.scope,
        serviceIdentifier: digest.serviceIdentifier,
        tagNames: [...digest.tagNames],
      })),
    };

    return JSON.stringify(payload);
  }

  public parse(serialized: string): ContainerSnapshot {
    const value: unknown = JSON.parse(serialized);

    if (!isSnapshotPayload(value)) {
      throw new Error('Supplied text is not a serialized snapshot payload.');
    }

    return this.restoreFromPayload(value);
  }

  public restoreFromPayload(payload: SnapshotPayload): ContainerSnapshot {
    const digests: BindingDigest[] = payload.digests.map(
      (digestPayload: DigestPayload) =>
        new BindingDigest({
          bindingType: digestPayload.bindingType,
          hasActivation: digestPayload.hasActivation,
          hasDeactivation: digestPayload.hasDeactivation,
          name: digestPayload.name,
          scope: digestPayload.scope,
          serviceIdentifier: digestPayload.serviceIdentifier,
          tagNames: [...digestPayload.tagNames],
        }),
    );

    return new ContainerSnapshot(
      payload.containerLabel,
      payload.defaultScope,
      digests,
    );
  }
}

function isSnapshotPayload(value: unknown): value is SnapshotPayload {
  if (typeof value !== 'object' || value === null) {
    return false;
  }

  const candidate: Record<string, unknown> = value as Record<string, unknown>;
  const knownScopes: readonly string[] = Object.values(bindingScopeValues);

  return (
    typeof candidate['containerLabel'] === 'string' &&
    typeof candidate['defaultScope'] === 'string' &&
    knownScopes.includes(candidate['defaultScope']) &&
    Array.isArray(candidate['digests'])
  );
}
