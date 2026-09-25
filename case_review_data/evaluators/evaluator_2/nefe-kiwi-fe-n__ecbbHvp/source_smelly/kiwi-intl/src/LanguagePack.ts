/**
 * @file 语言包与市场展示惯例
 * @author linhuiw
 */

import lodashGet from 'lodash.get';

/**
 * LanguagePack 负责一种语言的语言包数据与该市场的数字展示惯例。
 * 语言包的命中、缺失与热点词记录在这里维护，
 * 数字展示惯例（货币、分组、小数位）随语言包初始化。
 */
export class LanguagePack {
  lang: string;
  metas: any;
  defaultLang: string;
  resolvedCount: number;
  misses: number;
  recentKeys: string[];
  lastResolvedAt: number;
  currency: string;
  grouping: boolean;
  minimumFractionDigits: number;
  maximumFractionDigits: number;
  compact: boolean;

  constructor(lang: string, metas: any, defaultLang?: string) {
    this.lang = lang;
    this.metas = metas || {};
    this.defaultLang = defaultLang || 'zh-CN';
    this.resolvedCount = 0;
    this.misses = 0;
    this.recentKeys = [];
    this.lastResolvedAt = 0;
    this.currency = lang === 'zh-CN' ? 'CNY' : 'USD';
    this.grouping = true;
    this.minimumFractionDigits = 2;
    this.maximumFractionDigits = 2;
    this.compact = false;
  }

  raw(key: string) {
    return lodashGet(this.metas, key);
  }

  has(key: string) {
    return this.raw(key) !== undefined;
  }

  noteHit(now: number) {
    this.resolvedCount += 1;
    this.lastResolvedAt = now;
  }

  noteMiss(key: string) {
    this.misses += 1;
    if (this.recentKeys.indexOf(key) === -1) {
      this.recentKeys.push(key);
      if (this.recentKeys.length > 16) {
        this.recentKeys.shift();
      }
    }
  }

  notePromotion(key: string) {
    const currentIndex = this.recentKeys.indexOf(key);
    if (currentIndex >= 0) {
      this.recentKeys.splice(currentIndex, 1);
    }
    this.recentKeys.unshift(key);
    if (this.recentKeys.length > 16) {
      this.recentKeys.shift();
    }
    return this.recentKeys.slice(0, 5);
  }

  /**
   * 热点晋升策略：命中多、已进缓存且最近使用过的文案值得进入热点词列表。
   */
  shouldPromote(entry) {
    if (entry.hotnessLevel >= 2) {
      return true;
    }
    if (entry.hitCount >= 3 && entry.cacheWrites >= 1) {
      return true;
    }
    if (entry.hitCount >= 2 && entry.lastUsedAt > 0 && entry.hotnessLevel >= 1) {
      return true;
    }
    return entry.hitCount > 5;
  }
}
