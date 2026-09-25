import type { BindingScope, BindingType } from '@inversifyjs/core';

export interface BindingDigestDescriptor {
  readonly serviceIdentifier: string;
  readonly scope: BindingScope;
  readonly bindingType: BindingType;
  readonly hasActivation?: boolean | undefined;
  readonly hasDeactivation?: boolean | undefined;
  readonly name?: string | undefined;
  readonly tagNames?: readonly string[] | undefined;
}

export class BindingDigest {
  public readonly serviceIdentifier: string;
  public readonly scope: BindingScope;
  public readonly bindingType: BindingType;
  public readonly hasActivation: boolean;
  public readonly hasDeactivation: boolean;
  public readonly name: string | undefined;
  public readonly tagNames: readonly string[];

  constructor(descriptor: BindingDigestDescriptor) {
    this.serviceIdentifier = descriptor.serviceIdentifier;
    this.scope = descriptor.scope;
    this.bindingType = descriptor.bindingType;
    this.hasActivation = descriptor.hasActivation ?? false;
    this.hasDeactivation = descriptor.hasDeactivation ?? false;
    this.name = descriptor.name;
    this.tagNames = descriptor.tagNames ?? [];
  }
}
