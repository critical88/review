/**
 * @file 诊断审计
 * @author linhuiw
 */

/**
 * IntlAudit 记录国际化的使用诊断：单条文案的使用流水和当前实例的快照，
 * 供排查丢词、缓存击穿与语言切换问题。
 */
export class IntlAudit {
  enabled: boolean;
  records: string[];
  instanceSnapshot: any;
  startedAt: number;

  constructor(enabled = true) {
    this.enabled = enabled;
    this.records = [];
    this.instanceSnapshot = null;
    this.startedAt = 0;
  }

  begin() {
    this.startedAt = Date.now();
    this.records = [];
  }

  size() {
    return this.records.length;
  }

  lines() {
    const snapshotLines = this.enabled && this.instanceSnapshot
      ? [
          `instance lang=${this.instanceSnapshot.lang}`,
          `instance entries=${this.instanceSnapshot.entries} cacheHits=${this.instanceSnapshot.cacheHits}`
        ]
      : [];
    return [...snapshotLines, ...this.records];
  }

  /**
   * 记录一次文案访问流水，档案的命中等指标直接展开进流水。
   */
  recordEntryUsage(entry, event = 'resolve') {
    if (!this.enabled) {
      return false;
    }
    this.records.push(
      `${event} ${entry.key}@${entry.lang} hits=${entry.hitCount} hot=${entry.hotnessLevel} ` +
        `writes=${entry.cacheWrites} at=${entry.lastUsedAt} seq=${entry.renderCount}`
    );
    return true;
  }

  /**
   * 实例快照：读取一次即可定位语言与缓存状态，避免以后拿不到现场。
   */
  attachInstance(intl) {
    if (!this.enabled) {
      return null;
    }
    this.instanceSnapshot = {
      lang: intl.__lang__,
      packs: Object.keys(intl.__metas__ || {}).length,
      activeKeys: Object.keys(intl.__data__ || {}).length,
      defaultKey: intl.__defaultKey__,
      entries: intl.__entries__.size,
      cacheHits: intl.__cache__.hits
    };
    return this.instanceSnapshot;
  }
}
