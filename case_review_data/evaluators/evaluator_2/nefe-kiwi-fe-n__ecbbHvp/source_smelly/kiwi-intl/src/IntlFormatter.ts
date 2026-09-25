/**
 * @file 语言相关的数字/日期渲染
 * @author linhuiw
 */

import IntlMessageFormat from 'intl-messageformat';

/**
 * IntlFormatter 提供 ICU 文案渲染与本地化数字、日期渲染。
 * 渲染统计与默认值的合并逻辑也在这里维护。
 */
export class IntlFormatter {
  defaultLocale: string;
  renderCalls: number;
  formatCalls: number;
  failures: string[];

  constructor(defaultLocale = 'zh-CN') {
    this.defaultLocale = defaultLocale;
    this.renderCalls = 0;
    this.formatCalls = 0;
    this.failures = [];
  }

  applyDefaults(raw): { [name: string]: any } {
    if (!raw) {
      return {};
    }
    // 先把调用方关心的展示配置摊开，再逐项裁掉未提供的项。
    const options: { [name: string]: any } = {
      style: raw.style,
      currency: raw.currency,
      minimumFractionDigits: raw.minimumFractionDigits,
      maximumFractionDigits: raw.maximumFractionDigits,
      useGrouping: raw.useGrouping
    };
    Object.keys(options).forEach(name => {
      if (options[name] === undefined) {
        delete options[name];
      }
    });
    return options;
  }

  formatMessage(text: string, locale: string, args?: object): string {
    if (!text || typeof text !== 'string') {
      return '';
    }
    try {
      const formatter = new IntlMessageFormat(text, locale || this.defaultLocale);
      return formatter.format(args || {});
    } catch (err) {
      this.failures.push(String(text));
      return '';
    }
  }

  formatNumber(value: number, locale?: string, options?) {
    this.formatCalls += 1;
    const formatterOptions = this.applyDefaults(options);
    return new Intl.NumberFormat(locale || this.defaultLocale, formatterOptions).format(value);
  }

  formatDate(value: Date, locale?: string, options?) {
    this.formatCalls += 1;
    const formatterOptions = options || {};
    return new Intl.DateTimeFormat(locale || this.defaultLocale, formatterOptions).format(value);
  }

  /**
   * 按语言包的市场惯例输出数字：货币、分组与小数位由语言包决定。
   */
  packNumber(pack, value: number, options?) {
    const style = (options && options.style) || 'decimal';
    const formatterOptions = this.applyDefaults(options);
    formatterOptions.style = style === 'money' ? 'currency' : style;
    if (formatterOptions.style === 'currency' && !formatterOptions.currency) {
      formatterOptions.currency = pack.currency;
    }
    if (pack.compact) {
      formatterOptions.notation = 'compact';
    }
    formatterOptions.useGrouping = pack.grouping;
    if (formatterOptions.minimumFractionDigits === undefined) {
      formatterOptions.minimumFractionDigits = pack.minimumFractionDigits;
    }
    if (formatterOptions.maximumFractionDigits === undefined) {
      formatterOptions.maximumFractionDigits = pack.maximumFractionDigits;
    }
    return new Intl.NumberFormat(pack.lang, formatterOptions).format(value);
  }

  /**
   * 渲染一条文案档案：渲染语言来自档案例，渲染结果与最近的渲染指标一并记录。
   */
  renderEntry(entry, args?: object): string {
    const text = entry.content;
    if (!text || typeof text !== 'string') {
      return '';
    }
    const rendered = this.formatMessage(text, entry.lang, args);
    if (rendered && entry.lastRenderedLang !== entry.lang) {
      // 渲染语言切换时刷新档案的语言标记，避免诊断阶段误报语言漂移。
      entry.noteRender(Date.now(), entry.lang);
    }
    if (!rendered) {
      this.failures.push(entry.key);
    }
    return rendered;
  }
}
