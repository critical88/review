import { bindingScopeValues, bindingTypeValues } from '@inversifyjs/core';

import { BindingDigest } from './binding_digest';
import { ContainerSnapshot } from './container_snapshot';
import type { DigestLineWriter } from './digest_line_writer';

export class SnapshotTextReport {
  public render(snapshot: ContainerSnapshot): string {
    const lines: string[] = [
      `Container ${snapshot.containerLabel} wiring report`,
      `Default scope: ${snapshot.defaultScope}`,
    ];

    for (const digest of snapshot.digests) {
      lines.push(this.renderDigest(digest, '  '));
    }

    lines.push(`Registered services: ${snapshot.digests.length.toString()}`);

    return lines.join('\n');
  }

  public renderDigest(digest: BindingDigest, indentation: string): string {
    const parts: string[] = [`${indentation}${digest.serviceIdentifier}`];

    parts.push(`scope=${digest.scope}`);
    parts.push(`type=${digest.bindingType}`);

    if (digest.name !== undefined) {
      parts.push(`name=${digest.name}`);
    }

    if (digest.hasActivation) {
      parts.push('activation=registered');
    }

    parts.push(
      digest.hasDeactivation ? 'deactivation=registered' : 'deactivation=none',
    );

    parts.push(`tags=[${digest.tagNames.join(', ')}]`);

    return parts.join(' | ');
  }

  public renderBindingLegend(digest: BindingDigest): string {
    const legend: string[] = [`Service ${digest.serviceIdentifier}`];

    if (digest.scope === bindingScopeValues.Singleton) {
      legend.push('  one cached instance is reused for every resolution');
    } else if (digest.scope === bindingScopeValues.Request) {
      legend.push('  each resolution scope receives its own instance');
    } else {
      legend.push('  every resolution receives a new instance');
    }

    if (digest.bindingType === bindingTypeValues.Instance) {
      legend.push('  activates the constructor of the bound class');
    } else if (digest.bindingType === bindingTypeValues.ConstantValue) {
      legend.push('  resolves to the registered constant immediately');
    } else if (digest.bindingType === bindingTypeValues.Factory) {
      legend.push('  resolves through the registered factory');
    } else {
      legend.push('  resolves through the registered declaration');
    }

    if (digest.hasActivation || digest.hasDeactivation) {
      legend.push('  lifecycle handlers run around resolutions');
    }

    return legend.join('\n');
  }

  public renderInto(
    snapshot: ContainerSnapshot,
    writer: DigestLineWriter,
  ): void {
    for (const digest of snapshot.digests) {
      writer.writeLine(digest.serviceIdentifier);
    }
  }
}
