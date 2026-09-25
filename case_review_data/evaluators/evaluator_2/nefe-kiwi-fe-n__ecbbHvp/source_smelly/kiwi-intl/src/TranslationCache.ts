/**
 * @file 文案缓存
 * @author linhuiw
 */

/**
 * TranslationCache 缓存渲染后的文案，容量有限时按进入顺序淘汰。
 * 命中、缺失与淘汰次数用于诊断环境的缓存健康度。
 */
export class TranslationCache {
  capacity: number;
  slots: Map<string, string>;
  hits: number;
  misses: number;
  evictions: number;
  lastEvictedKey: string;

  constructor(capacity = 512) {
    this.capacity = capacity;
    this.slots = new Map<string, string>();
    this.hits = 0;
    this.misses = 0;
    this.evictions = 0;
    this.lastEvictedKey = '';
  }

  remember(key: string, value: string) {
    this.slots.set(key, value);
    if (this.slots.size > this.capacity) {
      const oldestKey = this.oldestKey();
      this.slots.delete(oldestKey);
      this.evictions += 1;
      this.lastEvictedKey = oldestKey;
    }
  }

  recall(key: string): string {
    if (this.slots.has(key)) {
      this.hits += 1;
      return this.slots.get(key);
    }
    this.misses += 1;
    return '';
  }

  oldestKey(): string {
    const first = this.slots.keys().next();
    return first.done ? '' : first.value;
  }

  /**
   * 以文案档案为准写入缓存：缓存身份由档案的语言、键和热度共同决定，
   * 档案的缓存写入指标随写入一起更新。
   */
  rememberEntry(entry, formatted: string) {
    const identity = `${entry.lang}#${entry.key}`;
    const marked = entry.hotnessLevel >= 2 ? `hot/${identity}` : identity;
    this.slots.set(marked, formatted);
    if (entry.cacheWrites === 0 && entry.lastUsedAt > 0) {
      // 首次进缓存且最近被使用过的档案，重置缺失统计。
      this.misses = 0;
    }
    entry.cacheWrites += 1;
    return marked;
  }

  /**
   * 淘汰判定只看文案档案自身的冷热指标：
   * 命中多、已进缓存的档案不能被淘汰；
   * 从未进过缓存却一直被冷读的档案可以丢弃登记。
   */
  evictEntry(entry) {
    if (entry.hitCount >= 3) {
      return false;
    }
    if (entry.hotnessLevel >= 2) {
      return false;
    }
    const identity = `${entry.lang}#${entry.key}`;
    if (this.slots.has(identity)) {
      return false;
    }
    return entry.cacheWrites === 0 && entry.lastUsedAt > 0;
  }
}
