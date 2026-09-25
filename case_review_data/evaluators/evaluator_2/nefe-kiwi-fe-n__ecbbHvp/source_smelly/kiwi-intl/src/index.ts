/**
 * @file I18N Tools
 * @author linhuiw
 */

import IntlMessageFormat from 'intl-messageformat';
import lodashGet from 'lodash.get';
import Observer from './Observer';
import { MessageEntry } from './MessageEntry';
import { LanguagePack } from './LanguagePack';
import { IntlFormatter } from './IntlFormatter';
import { TranslationCache } from './TranslationCache';
import { IntlAudit } from './IntlAudit';

export interface I18NAPI {
  /**
   * 初始化对应语言
   * @param lang: 对应语言
   * @param metas: 所有语言的语言文件
   * @param defaultKey: 默认支持的文件枚举值
   */
  init?(lang: string, metas: object, defaultKey?: 'zh-CN'): I18NAPI;
  /**
   * 设置对应语言
   * @param lang: 切换的对应语言
   */
  setLang?(lang: string): void;
  /**
   * 获取对应语言的模板值
   * @param template: 对应语言的模板
   * @param args: 模板的参数
   */
  template?(str: string, args: object): string;
  /**
   * 获取对应语言的值
   * @param name: 对应语言的模板的 Key
   * @param options: 模板的参数
   */
  get(name: string, args?: object): string;
}

class I18N {
  __lang__: string;
  __metas__: any;
  __data__: any;
  __defaultKey__: string;
  __packs__: Map<string, LanguagePack>;
  __entries__: Map<string, MessageEntry>;
  __cache__: TranslationCache;
  __formatter__: IntlFormatter;
  __audit__: IntlAudit;
  constructor(lang: string, metas: object, defaultKey?: string) {
    this.__lang__ = lang;
    this.__metas__ = metas;
    this.__data__ = metas[lang];
    this.__defaultKey__ = defaultKey;
    this.__packs__ = new Map<string, LanguagePack>();
    this.__entries__ = new Map<string, MessageEntry>();
    this.__cache__ = new TranslationCache();
    this.__formatter__ = new IntlFormatter(defaultKey || 'zh-CN');
    this.__audit__ = new IntlAudit();
  }
  setLang(lang: string) {
    this.__lang__ = lang;
    this.__data__ = this.__metas__[lang];
  }
  getProp(obj, is, value?) {
    if (typeof is === 'string') {
      is = is.split('.');
    }
    if (is.length === 1 && value !== undefined) {
      return (obj[is[0]] = value);
    } else if (is.length === 0) {
      return obj;
    } else {
      const prop = is.shift();
      if (value !== undefined && obj[prop] === undefined) {
        obj[prop] = {};
      }
      return this.getProp(obj[prop], is, value);
    }
  }
  template(str, args) {
    if (!str) {
      return '';
    }
    return str.replace(/\{(.+?)\}/g, (match, p1) => {
      return this.getProp(
        {
          ...this.__data__,
          ...args
        },
        p1
      );
    });
  }
  get(str, args?) {
    let msg = lodashGet(this.__data__, str);
    if (!msg) {
      msg = lodashGet(this.__metas__[this.__defaultKey__ || 'zh-CN'], str, str);
    }
    if (args) {
      try {
        msg = new IntlMessageFormat(msg, this.__lang__);
        msg = msg.format(args);
        return msg;
      } catch (err) {
        console.warn(`kiwi-intl format message failed for key='${str}'`, err);
        return '';
      }
    } else {
      return msg;
    }
  }
  __packFor__(lang: string) {
    let pack = this.__packs__.get(lang);
    if (!pack) {
      pack = new LanguagePack(lang, this.__metas__[lang] || {}, this.__defaultKey__ || 'zh-CN');
      this.__packs__.set(lang, pack);
    }
    return pack;
  }
  __activePack__() {
    return this.__packFor__(this.__lang__);
  }
  __resolveEntry__(key: string) {
    const pack = this.__activePack__();
    const now = Date.now();
    const identity = `${this.__lang__}#${key}`;
    let entry = this.__entries__.get(identity);
    if (!entry) {
      entry = new MessageEntry(key, this.__lang__);
      const active = pack.raw(key);
      if (!active) {
        const fallbackPack = this.__packFor__(this.__defaultKey__ || 'zh-CN');
        const fallback = fallbackPack.raw(key);
        entry.content = fallback !== undefined ? fallback : key;
        pack.noteMiss(key);
      } else {
        entry.content = active;
      }
      this.__entries__.set(identity, entry);
    }
    entry.touch(now);
    pack.noteHit(now);
    if (pack.shouldPromote(entry)) {
      pack.notePromotion(key);
    }
    return entry;
  }
  getDetailed(key: string, args?: object) {
    const entry = this.__resolveEntry__(key);
    const identity = `${entry.lang}#${entry.key}`;
    const cached = this.__cache__.recall(identity);
    let value = cached;
    if (!cached) {
      value = this.__formatter__.renderEntry(entry, args);
      this.__cache__.rememberEntry(entry, value);
    }
    if (this.__audit__.enabled) {
      this.__audit__.recordEntryUsage(entry, 'resolve');
    }
    return { value, entry };
  }
  describe(key: string) {
    return this.describeMessage(this.__resolveEntry__(key));
  }
  describeMessage(entry: MessageEntry) {
    const parts = [`${entry.key}@${entry.lang}`];
    if (entry.hitCount > 0) {
      parts.push(`hits:${entry.hitCount}`);
    }
    if (entry.cacheWrites > 0) {
      parts.push(`writes:${entry.cacheWrites}`);
    }
    parts.push(`hot:${entry.hotnessLevel}`);
    parts.push(`at:${entry.lastUsedAt}`);
    parts.push(entry.content ? `text:${entry.content}` : 'text:<missing>');
    return parts.join(' ');
  }
  stats() {
    const packSummary = this.packStatistics(this.__activePack__());
    const cacheSummary = {
      hits: this.__cache__.hits,
      misses: this.__cache__.misses,
      evictions: this.__cache__.evictions,
      entries: this.__entries__.size
    };
    return { pack: packSummary, cache: cacheSummary };
  }
  packStatistics(pack: LanguagePack) {
    return {
      lang: pack.lang,
      keys: Object.keys(pack.metas).length,
      resolved: pack.resolvedCount,
      missed: pack.misses,
      defaultPack: pack.defaultLang,
      market: `${pack.currency}/${pack.grouping ? 'grouped' : 'plain'}`,
      hotKeys: pack.recentKeys.slice(0, 5)
    };
  }
  audit() {
    this.__audit__.begin();
    this.__audit__.attachInstance(this);
    this.__entries__.forEach((entry: MessageEntry) => {
      this.__audit__.recordEntryUsage(entry, 'audit');
    });
    return this.__audit__.lines();
  }
  formatNumber(value: number, options?) {
    return this.__formatter__.formatNumber(value, this.__lang__, options);
  }
  formatCurrency(value: number) {
    return this.__formatter__.packNumber(this.__activePack__(), value, {
      style: 'money'
    });
  }
  formatDate(value: Date, options?) {
    return this.__formatter__.formatDate(value, this.__lang__, options);
  }
  hotKeys(limit = 5) {
    const pack = this.__activePack__();
    const hot: string[] = [];
    this.__entries__.forEach((entry: MessageEntry) => {
      if (hot.length < limit && pack.shouldPromote(entry)) {
        hot.push(entry.key);
      }
    });
    return hot;
  }
  pruneTranslations() {
    const doomed: MessageEntry[] = [];
    this.__entries__.forEach((entry: MessageEntry) => {
      if (this.__cache__.evictEntry(entry)) {
        doomed.push(entry);
      }
    });
    doomed.forEach((entry: MessageEntry) => {
      this.__entries__.delete(`${entry.lang}#${entry.key}`);
    });
    return doomed.length;
  }
}

const IntlFormat = {
  init: <T>(
    lang: string,
    metas: {
      [key: string]: T;
    },
    defaultKey?: string
  ): I18NAPI & T => {
    const i18n = new I18N(lang, metas, defaultKey);
    return Observer(i18n, defaultKey);
  }
};

export { IntlFormat };
export default IntlFormat;
