import 'reflect-metadata';

import { expect } from 'chai';

import {
  BindingDigest,
  bindingScopeValues,
  bindingTypeValues,
  Container,
  ContainerSnapshot,
  SnapshotCodec,
  SnapshotComparator,
  SnapshotStatistics,
  SnapshotTextReport,
  StringDigestLineWriter,
  WiringAudit,
} from '../..';

describe('Container diagnostics', () => {
  const katanaDigest: BindingDigest = new BindingDigest({
    bindingType: bindingTypeValues.Instance,
    hasActivation: true,
    name: 'primary',
    scope: bindingScopeValues.Singleton,
    serviceIdentifier: 'Katana',
    tagNames: ['sharp'],
  });

  const shurikenDigest: BindingDigest = new BindingDigest({
    bindingType: bindingTypeValues.ConstantValue,
    scope: bindingScopeValues.Transient,
    serviceIdentifier: 'Shuriken',
  });

  const snapshot: ContainerSnapshot = new ContainerSnapshot(
    'armory',
    bindingScopeValues.Singleton,
    [katanaDigest, shurikenDigest],
  );

  describe('Plain text report', () => {
    it('Should render the wiring report of a snapshot', () => {
      const report: SnapshotTextReport = new SnapshotTextReport();

      expect(report.render(snapshot)).equal(
        [
          'Container armory wiring report',
          'Default scope: Singleton',
          '  Katana | scope=Singleton | type=Instance | name=primary | activation=registered | deactivation=none | tags=[sharp]',
          '  Shuriken | scope=Transient | type=ConstantValue | deactivation=none | tags=[]',
          'Registered services: 2',
        ].join('\n'),
      );
    });

    it('Should stream the registered identifiers into a line writer', () => {
      const writer: StringDigestLineWriter = new StringDigestLineWriter();
      const report: SnapshotTextReport = new SnapshotTextReport();

      report.renderInto(snapshot, writer);

      expect(writer.render()).equal(['Katana', 'Shuriken'].join('\n'));
    });

    it('Should explain what a registration means for resolution', () => {
      const report: SnapshotTextReport = new SnapshotTextReport();

      expect(report.renderBindingLegend(katanaDigest)).equal(
        [
          'Service Katana',
          '  one cached instance is reused for every resolution',
          '  activates the constructor of the bound class',
          '  lifecycle handlers run around resolutions',
        ].join('\n'),
      );
    });
  });

  describe('Snapshot statistics', () => {
    it('Should break registrations down by scope and binding type', () => {
      const statistics: SnapshotStatistics = new SnapshotStatistics();
      const breakdown: Map<string, number> =
        statistics.computeScopeBreakdown(snapshot);

      expect(breakdown.get('Singleton/Instance+lifecycle')).equal(1);
      expect(breakdown.get('Transient/ConstantValue')).equal(1);
      expect(breakdown.size).equal(2);
    });

    it('Should count registrations that keep instances or run lifecycle handlers', () => {
      const statistics: SnapshotStatistics = new SnapshotStatistics();

      expect(statistics.countStatefulRegistrations(snapshot)).equal(2);
      expect(
        statistics.countStatefulRegistrations(
          new ContainerSnapshot('empty', bindingScopeValues.Request),
        ),
      ).equal(0);
    });

    it('Should count qualifier usage across registrations', () => {
      const statistics: SnapshotStatistics = new SnapshotStatistics();
      const usage: Map<string, number> = statistics.computeTagUsage(snapshot);

      expect(usage.get('sharp')).equal(1);
      expect(usage.get('name:primary')).equal(1);
      expect(usage.get('(unqualified)')).equal(1);

      const qualifiedOnly: Map<string, number> = statistics.computeTagUsage(
        new ContainerSnapshot('qualified', bindingScopeValues.Singleton, [
          katanaDigest,
          katanaDigest,
        ]),
      );

      expect(qualifiedOnly.get('sharp')).equal(2);
      expect(qualifiedOnly.get('name:primary')).equal(2);
      expect(qualifiedOnly.has('(unqualified)')).equal(false);
    });
  });

  describe('Snapshot comparison', () => {
    it('Should describe scope changes between two snapshots', () => {
      const updated: ContainerSnapshot = new ContainerSnapshot(
        'armory',
        bindingScopeValues.Singleton,
        [
          new BindingDigest({
            bindingType: bindingTypeValues.Instance,
            hasActivation: true,
            name: 'primary',
            scope: bindingScopeValues.Request,
            serviceIdentifier: 'Katana',
            tagNames: ['sharp'],
          }),
          shurikenDigest,
        ],
      );

      const differences: SnapshotComparator = new SnapshotComparator();

      expect(differences.diff(snapshot, updated)).deep.equal([
        {
          description: 'scope: Singleton -> Request',
          kind: 'registration',
          serviceIdentifier: 'Katana',
        },
      ]);
    });

    it('Should report registrations missing from the baseline as added', () => {
      const extended: ContainerSnapshot = new ContainerSnapshot(
        'armory',
        bindingScopeValues.Singleton,
        [
          katanaDigest,
          shurikenDigest,
          new BindingDigest({
            bindingType: bindingTypeValues.Instance,
            scope: bindingScopeValues.Transient,
            serviceIdentifier: 'Tanto',
          }),
        ],
      );

      const differences: SnapshotComparator = new SnapshotComparator();

      expect(differences.diff(snapshot, extended)).deep.equal([
        {
          description: 'registered after baseline (scope=Transient, Instance)',
          kind: 'added',
          serviceIdentifier: 'Tanto',
        },
      ]);
    });

    it('Should report unchanged snapshots without differences', () => {
      const differences: SnapshotComparator = new SnapshotComparator();

      expect(differences.diff(snapshot, snapshot)).deep.equal([]);
    });
  });

  describe('Snapshot serialization', () => {
    it('Should serialize and parse a snapshot without losing wiring details', () => {
      const codec: SnapshotCodec = new SnapshotCodec();
      const serialized: string = codec.serialize(snapshot);
      const parsed: ContainerSnapshot = codec.parse(serialized);
      const report: SnapshotTextReport = new SnapshotTextReport();

      expect(report.render(parsed)).equal(report.render(snapshot));
      expect(new SnapshotComparator().diff(snapshot, parsed)).deep.equal([]);
    });

    it('Should reject text that is not a serialized snapshot', () => {
      const codec: SnapshotCodec = new SnapshotCodec();

      expect(() => codec.parse('{"containerLabel":"armory"}')).throw(
        'Supplied text is not a serialized snapshot payload.',
      );
    });
  });

  describe('Live container audit', () => {
    it('Should list registrations with no matching binding in the container', () => {
      class Katana {}

      const container: Container = new Container();
      container.bind<Katana>('Katana').to(Katana);

      const audit: WiringAudit = new WiringAudit(container);

      expect(audit.listUnboundRegistrations(snapshot)).deep.equal(['Shuriken']);
    });

    it('Should describe the runtime status of a registration including lifecycle handlers', () => {
      class Katana {}

      const container: Container = new Container();
      container.bind<Katana>('Katana').to(Katana);

      const audit: WiringAudit = new WiringAudit(container);

      expect(audit.describeRuntimeStatus(katanaDigest)).equal(
        'Katana: bound (Singleton/Instance) activation registered, no deactivation',
      );
      expect(audit.describeRuntimeStatus(shurikenDigest)).equal(
        'Shuriken: unbound (Transient/ConstantValue) no activation, no deactivation',
      );
    });
  });
});
