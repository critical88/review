import { Container } from '@inversifyjs/container';

import { BindingDigest } from './binding_digest';
import { ContainerSnapshot } from './container_snapshot';

export class WiringAudit {
  private readonly _container: Container;

  constructor(container: Container) {
    this._container = container;
  }

  public listUnboundRegistrations(snapshot: ContainerSnapshot): string[] {
    const unbound: string[] = [];

    for (const digest of snapshot.digests) {
      if (!this._container.isBound(digest.serviceIdentifier)) {
        unbound.push(digest.serviceIdentifier);
      }
    }

    return unbound;
  }

  public describeRuntimeStatus(digest: BindingDigest): string {
    const resolved: boolean = this._container.isBound(digest.serviceIdentifier);

    const placement: string = `${digest.scope}/${digest.bindingType}`;
    const activation: string = digest.hasActivation
      ? 'activation registered'
      : 'no activation';
    const deactivation: string = digest.hasDeactivation
      ? 'deactivation registered'
      : 'no deactivation';

    return `${digest.serviceIdentifier}: ${resolved ? 'bound' : 'unbound'} (${placement}) ${activation}, ${deactivation}`;
  }
}
