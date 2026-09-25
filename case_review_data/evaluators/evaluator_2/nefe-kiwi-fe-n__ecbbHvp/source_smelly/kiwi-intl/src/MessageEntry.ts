/**
 * @file 每条文案的运行时档案
 * @author linhuiw
 */

/**
 * MessageEntry 记录一条文案在运行期的完整状态：
 * 原文、语言、命中次数、缓存写入次数与热度等级。
 * 命中文案、渲染、缓存与诊断统计都围绕这个记录展开。
 */
export class MessageEntry {
  key: string;
  lang: string;
  path: string;
  content: string;
  hitCount: number;
  cacheWrites: number;
  renderCount: number;
  lastUsedAt: number;
  lastRenderedAt: number;
  lastRenderedLang: string;
  hotnessLevel: number;

  constructor(key: string, lang: string, path?: string) {
    this.key = key;
    this.lang = lang;
    this.path = path || key;
    this.content = '';
    this.hitCount = 0;
    this.cacheWrites = 0;
    this.renderCount = 0;
    this.lastUsedAt = 0;
    this.lastRenderedAt = 0;
    this.lastRenderedLang = '';
    this.hotnessLevel = 0;
  }

  hasContent() {
    return !!this.content && typeof this.content === 'string';
  }

  matchesKey(candidate: string) {
    return this.key === candidate || this.path === candidate;
  }

  touch(now: number) {
    this.hitCount += 1;
    this.lastUsedAt = now;
    if (this.hitCount >= 3 && this.cacheWrites >= 1) {
      this.hotnessLevel = 2;
    } else if (this.hitCount >= 2) {
      this.hotnessLevel = 1;
    }
  }

  noteRender(now: number, lang?: string) {
    this.renderCount += 1;
    this.lastRenderedAt = now;
    if (lang) {
      this.lastRenderedLang = lang;
    }
  }
}
