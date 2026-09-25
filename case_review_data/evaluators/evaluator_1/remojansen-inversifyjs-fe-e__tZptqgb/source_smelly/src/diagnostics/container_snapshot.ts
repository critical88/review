import type { BindingScope } from '@inversifyjs/core';

import { BindingDigest } from './binding_digest';

export class ContainerSnapshot {
  public readonly containerLabel: string;
  public readonly defaultScope: BindingScope;
  public readonly digests: readonly BindingDigest[];

  constructor(
    containerLabel: string,
    defaultScope: BindingScope,
    digests: readonly BindingDigest[] = [],
  ) {
    this.containerLabel = containerLabel;
    this.defaultScope = defaultScope;
    this.digests = [...digests];
  }
}
