import * as assert from 'assert';
import IntlFormat from '../src/index';
import { getDefaultProxyString, getProxyObj } from '../src/utils';
import { MessageEntry } from '../src/MessageEntry';
import { LanguagePack } from '../src/LanguagePack';
import { IntlFormatter } from '../src/IntlFormatter';
import { TranslationCache } from '../src/TranslationCache';
import { IntlAudit } from '../src/IntlAudit';

const METAS = {
  'zh-CN': {
    value: '值',
    test: '测试',
    testTemplate: '你有{value}条未读通知',
    multiTemplate: '{value}条来自{name}的未读通知',
    deepTemplate: '深层值：{deep.a.b}',
    genderTemplate: '{gender, select, male {他} female {她} other {ta}}',
    numberTemplate: '计数：{count, number}',
    empty: '',
    countLiteral: 3,
    foo: {
      bar: 'foobar'
    },
    config: {
      theme: '浅色'
    },
    deep: {
      a: {
        b: '深层的值'
      }
    },
    photo: '我{num, plural, =0 {没有照片} =1 {有1张照片} other {有#张照片}}'
  },
  'en-US': {
    value: 'value',
    greeting: 'Hello {name}',
    photo: 'You have {num, plural, =0 {no photos} =1 {one photo} other {# photos}}.',
    quantity: 'There are {num, plural, =0 {no items} other {# items}} in the box.',
    config: {
      theme: 'dark'
    }
  },
  'fr-FR': {
    value: 'valeur'
  }
};

const zhCNIntl: any = IntlFormat.init('zh-CN', METAS);
const enUSIntl: any = IntlFormat.init('en-US', METAS);

describe('IntlFormat', function() {
  describe('init', function() {
    it('exposes the public i18n api', function() {
      assert.strictEqual(typeof zhCNIntl.get, 'function');
      assert.strictEqual(typeof zhCNIntl.template, 'function');
      assert.strictEqual(typeof zhCNIntl.setLang, 'function');
    });
    it('creates independent instances from separate init calls', function() {
      const first: any = IntlFormat.init('zh-CN', {
        'zh-CN': { value: 'firstvalue' }
      });
      const second: any = IntlFormat.init('zh-CN', {
        'zh-CN': { value: 'secondvalue' }
      });
      assert.strictEqual(first.get('value'), 'firstvalue');
      assert.strictEqual(second.get('value'), 'secondvalue');
    });
    it('starts lookups from the requested language', function() {
      assert.strictEqual(enUSIntl.get('value'), 'value');
      assert.strictEqual(zhCNIntl.get('value'), '值');
    });
    it('falls back to zh-CN when the active language lacks a key', function() {
      assert.strictEqual(enUSIntl.get('test'), '测试');
    });
    it('honors an explicit defaultKey fallback pack', function() {
      const twIntl: any = IntlFormat.init('fr-FR', METAS, 'en-US');
      assert.strictEqual(twIntl.get('value'), 'valeur');
      assert.strictEqual(twIntl.get('greeting'), 'Hello {name}');
    });
    it('supports an empty message value through the fallback chain', function() {
      const value = zhCNIntl.get('value');
      assert.strictEqual(zhCNIntl.get('empty'), '');
      assert.strictEqual(value, '值');
    });
  });

  describe('get', function() {
    it('returns the translated value for the active language', function() {
      assert.strictEqual(zhCNIntl.get('test'), '测试');
      assert.strictEqual(enUSIntl.get('value'), 'value');
    });
    it('returns deep values with dot notation', function() {
      assert.strictEqual(zhCNIntl.get('foo.bar'), 'foobar');
      assert.strictEqual(zhCNIntl.get('deep.a.b'), '深层的值');
    });
    it('returns the key itself when no pack defines it', function() {
      assert.strictEqual(zhCNIntl.get('noSuchKeyAnywhere'), 'noSuchKeyAnywhere');
      assert.strictEqual(enUSIntl.get('deep.a.b.x'), 'deep.a.b.x');
    });
    it('returns the raw template string when no args are given', function() {
      assert.strictEqual(zhCNIntl.get('testTemplate'), '你有{value}条未读通知');
      assert.strictEqual(enUSIntl.get('photo'), 'You have {num, plural, =0 {no photos} =1 {one photo} other {# photos}}.');
    });
    it('falls back to the default pack for deep keys', function() {
      assert.strictEqual(enUSIntl.get('foo.bar'), 'foobar');
    });
    it('returns non string message values untouched without args', function() {
      assert.strictEqual(zhCNIntl.get('countLiteral'), 3);
    });
    it('formats an ICU plural message with zero', function() {
      assert.strictEqual(zhCNIntl.get('photo', { num: 0 }), '我没有照片');
    });
    it('formats an ICU plural message with one', function() {
      assert.strictEqual(zhCNIntl.get('photo', { num: 1 }), '我有1张照片');
    });
    it('formats an ICU plural message with other and grouping', function() {
      assert.strictEqual(zhCNIntl.get('photo', { num: 1000 }), '我有1,000张照片');
    });
    it('formats ICU plural messages in the active language', function() {
      assert.strictEqual(enUSIntl.get('quantity', { num: 0 }), 'There are no items in the box.');
      assert.strictEqual(enUSIntl.get('quantity', { num: 42 }), 'There are 42 items in the box.');
    });
    it('formats an ICU select message', function() {
      assert.strictEqual(zhCNIntl.get('genderTemplate', { gender: 'male' }), '他');
      assert.strictEqual(zhCNIntl.get('genderTemplate', { gender: 'female' }), '她');
      assert.strictEqual(zhCNIntl.get('genderTemplate', { gender: 'robot' }), 'ta');
    });
    it('formats an ICU number argument', function() {
      assert.strictEqual(zhCNIntl.get('numberTemplate', { count: 1234.5 }), '计数：1,234.5');
    });
    it('interpolates simple placeholder args', function() {
      assert.strictEqual(enUSIntl.get('greeting', { name: 'kiwi' }), 'Hello kiwi');
    });
    it('ignores extra args that the message does not reference', function() {
      assert.strictEqual(
        enUSIntl.get('greeting', { name: 'kiwi', unused: 'x' }),
        'Hello kiwi'
      );
    });
    it('returns an empty string when formatting a non template object value', function() {
      assert.strictEqual(zhCNIntl.get('foo', { x: 1 }), '');
    });
    it('formats the key literal when a missing key gets args', function() {
      assert.strictEqual(zhCNIntl.get('missing.everywhere', { x: 1 }), 'missing.everywhere');
    });
    it('lets an empty fallback-pack value win over the key literal', function() {
      assert.strictEqual(enUSIntl.get('empty'), '');
    });
    it('scales an ICU number argument into the thousands', function() {
      assert.strictEqual(zhCNIntl.get('numberTemplate', { count: 1000000 }), '计数：1,000,000');
    });
    it('treats an empty key lookup as missing', function() {
      assert.strictEqual(zhCNIntl.get(''), '');
    });
  });

  describe('template', function() {
    it('substitutes args into the provided template', function() {
      assert.strictEqual(
        zhCNIntl.template('你有{value}条未读通知', { value: 3 }),
        '你有3条未读通知'
      );
    });
    it('resolves deep data keys inside placeholders', function() {
      assert.strictEqual(
        zhCNIntl.template('深层值：{deep.a.b}', {}),
        '深层值：深层的值'
      );
      assert.strictEqual(
        zhCNIntl.template(zhCNIntl.get('deepTemplate'), { }),
        '深层值：深层的值'
      );
    });
    it('merges active language data with the passed args', function() {
      assert.strictEqual(
        zhCNIntl.template('{value}条来自{name}的未读通知', { name: 'kiwi' }),
        '值条来自kiwi的未读通知'
      );
    });
    it('prefers args over message data for the same placeholder', function() {
      assert.strictEqual(
        zhCNIntl.template('{value}条未读', { value: 7 }),
        '7条未读'
      );
    });
    it('renders undefined for placeholders that resolve nowhere', function() {
      assert.strictEqual(
        zhCNIntl.template('你有{fictional}条未读通知', {}),
        '你有undefined条未读通知'
      );
    });
    it('leaves literal text without placeholders untouched', function() {
      assert.strictEqual(zhCNIntl.template('纯文本', {}), '纯文本');
    });
    it('returns an empty string for empty or missing templates', function() {
      assert.strictEqual(zhCNIntl.template('', {}), '');
      assert.strictEqual((zhCNIntl.template as any)(), '');
      assert.strictEqual((zhCNIntl.template as any)(null, {}), '');
    });
    it('replaces multiple placeholders in one template', function() {
      assert.strictEqual(
        zhCNIntl.template(zhCNIntl.get('multiTemplate'), { value: 2, name: 'kiwi' }),
        '2条来自kiwi的未读通知'
      );
    });
    it('only substitutes the first lazy brace group of doubled braces', function() {
      assert.strictEqual(zhCNIntl.template('{{value}}', { value: 1 }), 'undefined}');
    });
    it('lets arg objects replace whole data subtrees in placeholders', function() {
      assert.strictEqual(
        zhCNIntl.template('深层值：{deep.a.b}', { deep: { a: { b: '替换值' } } }),
        '深层值：替换值'
      );
    });
    it('resolves placeholders against the active language after setLang', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      intl.setLang('en-US');
      assert.strictEqual(intl.template('{value} unread', {}), 'value unread');
      intl.setLang('zh-CN');
      assert.strictEqual(intl.template('{value} unread', {}), '值 unread');
    });
  });

  describe('setLang', function() {
    let intl: any;
    beforeEach(function() {
      intl = IntlFormat.init('zh-CN', METAS);
    });
    it('switches the active language for get', function() {
      intl.setLang('en-US');
      assert.strictEqual(intl.get('value'), 'value');
    });
    it('keeps the default pack fallback after switching', function() {
      intl.setLang('fr-FR');
      assert.strictEqual(intl.get('value'), 'valeur');
      assert.strictEqual(intl.get('test'), '测试');
      assert.strictEqual(intl.get('foo.bar'), 'foobar');
    });
    it('falls back to the default pack when the switch has no data', function() {
      intl.setLang('xx-YY');
      assert.strictEqual(intl.get('value'), '值');
      assert.strictEqual(intl.get('photo', { num: 2 }), '我有2张照片');
    });
    it('supports switching back and forth', function() {
      intl.setLang('en-US');
      assert.strictEqual(intl.get('value'), 'value');
      intl.setLang('zh-CN');
      assert.strictEqual(intl.get('value'), '值');
      assert.strictEqual(intl.get('photo', { num: 0 }), '我没有照片');
    });
    it('affects the language used to format ICU messages', function() {
      intl.setLang('en-US');
      assert.strictEqual(intl.get('photo', { num: 0 }), 'You have no photos.');
      assert.strictEqual(intl.get('photo', { num: 5 }), 'You have 5 photos.');
    });
  });

  describe('reactive property access', function() {
    let intl: any;
    beforeEach(function() {
      intl = IntlFormat.init('zh-CN', METAS);
    });
    it('reads deep object values through the reactive proxy', function() {
      assert.strictEqual(intl.foo.bar, 'foobar');
      assert.strictEqual(intl.deep.a.b, '深层的值');
    });
    it('returns an empty proxy string for defined but empty keys', function() {
      assert.strictEqual(`${intl.empty}`, '');
      assert.strictEqual(intl.empty.valueOf(), '');
    });
    it('returns plain undefined for keys outside the message packs', function() {
      assert.strictEqual(intl.absentKey, undefined);
    });
    it('returns an empty proxy string for missing nested keys', function() {
      assert.strictEqual(`${intl.foo.missingNested}`, '');
      assert.strictEqual(`${intl.deep.missing.a}`, '');
    });
    it('falls back to the default pack for reactive reads', function() {
      intl.setLang('fr-FR');
      assert.strictEqual(intl.foo.bar, 'foobar');
    });
    it('serves reactive object reads from the active language', function() {
      intl.setLang('en-US');
      assert.strictEqual(intl.config.theme, 'dark');
    });
    it('falls back to the default pack for reactive object reads', function() {
      intl.setLang('fr-FR');
      assert.strictEqual(intl.config.theme, '浅色');
    });
    it('keeps the message data observable per instance', function() {
      const other: any = IntlFormat.init('zh-CN', {
        'zh-CN': { value: '另一个值' }
      });
      assert.strictEqual(intl.get('value'), '值');
      assert.strictEqual(other.get('value'), '另一个值');
      intl.setLang('fr-FR');
      assert.strictEqual(other.get('value'), '另一个值');
    });
  });

  describe('proxy utils', function() {
    it('getDefaultProxyString keeps the wrapped string value', function() {
      const proxied = getDefaultProxyString('abc');
      assert.strictEqual(proxied.valueOf(), 'abc');
      assert.strictEqual(proxied.toString(), 'abc');
      assert.strictEqual(`${proxied}`, 'abc');
    });
    it('getDefaultProxyString returns an empty string by default', function() {
      const proxied = getDefaultProxyString();
      assert.strictEqual(proxied.valueOf(), '');
      assert.strictEqual(`${proxied}`, '');
    });
    it('getDefaultProxyString keeps String object methods and properties', function() {
      const proxied = getDefaultProxyString('abcd');
      assert.strictEqual(proxied.length, 4);
      assert.strictEqual(proxied.toUpperCase(), 'ABCD');
    });
    it('getDefaultProxyString chains missing property reads', function() {
      const proxied: any = getDefaultProxyString('abc');
      assert.strictEqual(`${proxied.anything}`, '');
      assert.strictEqual(`${proxied.a.b.c}`, '');
    });
    it('getProxyObj wraps nested object values', function() {
      const proxied: any = getProxyObj(METAS);
      assert.strictEqual(proxied['zh-CN'].test, '测试');
      assert.strictEqual(proxied['zh-CN'].deep.a.b, '深层的值');
    });
    it('getProxyObj returns an empty proxy string for missing keys', function() {
      const proxied: any = getProxyObj({ a: 1 });
      assert.strictEqual(`${proxied.missing}`, '');
      assert.strictEqual(proxied.missing.valueOf(), '');
    });
    it('getProxyObj preserves raw primitive values', function() {
      const proxied: any = getProxyObj({ n: 5, s: 'text', f: false });
      assert.strictEqual(proxied.n, 5);
      assert.strictEqual(proxied.s, 'text');
      assert.strictEqual(proxied.f, false);
    });
    it('getProxyObj follows into arrays as objects', function() {
      const proxied: any = getProxyObj({ list: [1, 2] });
      assert.strictEqual(proxied.list[0], 1);
      assert.strictEqual(proxied.list[1], 2);
    });
    it('getProxyObj keeps array length and tools', function() {
      const proxied: any = getProxyObj({ list: [1, 2] });
      assert.strictEqual(proxied.list.length, 2);
      assert.strictEqual(proxied.list.join('-'), '1-2');
    });
    it('getProxyObj returns empty proxy strings beyond array bounds', function() {
      const proxied: any = getProxyObj([1, 2]);
      assert.strictEqual(`${proxied[10]}`, '');
      assert.strictEqual(proxied[0], 1);
    });
  });

  describe('getDetailed pipeline', function() {
    it('resolves the current-language message and exposes its entry', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      const detail: any = intl.getDetailed('value');
      assert.strictEqual(detail.value, '值');
      assert.strictEqual(detail.entry.key, 'value');
      assert.strictEqual(detail.entry.lang, 'zh-CN');
      assert.strictEqual(detail.entry.content, '值');
    });
    it('formats ICU templates through the render step', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      const detail: any = intl.getDetailed('testTemplate', { value: 7 });
      assert.strictEqual(detail.value, '你有7条未读通知');
    });
    it('falls back to the default pack for content in detailed reads', function() {
      const intl: any = IntlFormat.init('en-US', METAS, 'zh-CN');
      const detail: any = intl.getDetailed('test');
      assert.strictEqual(detail.entry.content, '测试');
      assert.strictEqual(detail.value, '测试');
    });
    it('keeps one entry per key and reuses it across reads', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      const first: any = intl.getDetailed('value');
      const second: any = intl.getDetailed('value');
      assert.strictEqual(first.entry === second.entry, true);
      assert.strictEqual(second.entry.hitCount, 2);
      assert.strictEqual(second.entry.cacheWrites, 1);
    });
    it('serves repeated reads from the render cache', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      intl.getDetailed('test');
      const again: any = intl.getDetailed('test');
      assert.strictEqual(again.value, '测试');
      assert.strictEqual(again.entry.cacheWrites, 1);
      assert.strictEqual(intl.__cache__.hits, 1);
    });
    it('keeps entry registries per instance', function() {
      const a: any = IntlFormat.init('zh-CN', METAS);
      const b: any = IntlFormat.init('zh-CN', METAS);
      a.getDetailed('value');
      b.getDetailed('value');
      assert.strictEqual(a.__entries__.size, 1);
      assert.strictEqual(b.__entries__.size, 1);
      assert.strictEqual(
        a.getDetailed('value').entry === b.getDetailed('value').entry,
        false
      );
    });
    it('records unusable templates as render failures', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      const detail: any = intl.getDetailed('oops{');
      assert.strictEqual(detail.value, '');
      assert.strictEqual(
        intl.__formatter__.failures.indexOf('oops{') !== -1,
        true
      );
    });
  });

  describe('message entry metrics', function() {
    it('raises the hotness level with repeated use and cache writes', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      intl.getDetailed('value');
      assert.strictEqual(intl.getDetailed('value').entry.hotnessLevel, 1);
      assert.strictEqual(intl.getDetailed('value').entry.hotnessLevel, 2);
      assert.strictEqual(intl.getDetailed('value').entry.hotnessLevel, 2);
    });
    it('tracks render bookkeeping on the entry', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      const detail: any = intl.getDetailed('photo', { num: 2 });
      assert.strictEqual(detail.entry.renderCount, 1);
      assert.strictEqual(detail.entry.lastRenderedLang, 'zh-CN');
      intl.getDetailed('photo', { num: 3 });
      assert.strictEqual(detail.entry.renderCount, 1);
    });
  });

  describe('describe & stats', function() {
    it('summarises a key with its usage metrics', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      intl.getDetailed('value');
      intl.getDetailed('value');
      const line = intl.describe('value');
      assert.strictEqual(line.indexOf('value@zh-CN'), 0);
      assert.strictEqual(line.indexOf('hits:3') !== -1, true);
      assert.strictEqual(line.indexOf('writes:1') !== -1, true);
      assert.strictEqual(line.indexOf('hot:2') !== -1, true);
      assert.strictEqual(line.indexOf('text:值') !== -1, true);
    });
    it('describes unsupported keys by mirroring them as content', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      const line = intl.describe('no-such-key');
      assert.strictEqual(line.indexOf('no-such-key@zh-CN'), 0);
      assert.strictEqual(line.indexOf('text:no-such-key') !== -1, true);
      const empty = intl.describe('empty');
      assert.strictEqual(empty.indexOf('text:<missing>') !== -1, true);
    });
    it('reports pack resolution counters', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      intl.getDetailed('value');
      intl.getDetailed('test');
      const s: any = intl.stats();
      assert.strictEqual(s.pack.lang, 'zh-CN');
      assert.strictEqual(s.pack.resolved, 2);
      assert.strictEqual(s.pack.missed, 0);
      assert.strictEqual(s.pack.keys, Object.keys(METAS['zh-CN']).length);
      assert.strictEqual(s.pack.market, 'CNY/grouped');
      assert.strictEqual(s.pack.defaultPack, 'zh-CN');
    });
    it('counts cache hits, misses and entries', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      intl.getDetailed('value');
      intl.getDetailed('value');
      const s: any = intl.stats();
      assert.strictEqual(s.cache.hits, 1);
      // 第一次记缺后立即写入缓存，未清零的缺失计数由建设性写入复位。
      assert.strictEqual(s.cache.misses, 0);
      assert.strictEqual(s.cache.evictions, 0);
      assert.strictEqual(s.cache.entries, 1);
    });
    it('labels misses for keys absent from the active pack', function() {
      const intl: any = IntlFormat.init('en-US', METAS, 'zh-CN');
      intl.getDetailed('greeting');
      assert.strictEqual(intl.stats().missedWrapper === undefined, true);
      const s: any = intl.stats();
      assert.strictEqual(s.pack.lang, 'en-US');
      assert.strictEqual(s.pack.missed, 0);
      intl.getDetailed('test');
      const after: any = intl.stats();
      assert.strictEqual(after.pack.missed, 1);
      assert.strictEqual(after.pack.resolved, 2);
    });
  });

  describe('audit journal', function() {
    it('snapshots the instance and journals each entry once', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      intl.getDetailed('value');
      const lines: string[] = intl.audit();
      assert.strictEqual(lines[0], 'instance lang=zh-CN');
      assert.strictEqual(lines[1], 'instance entries=1 cacheHits=0');
      assert.strictEqual(lines[2].indexOf('audit value@zh-CN'), 0);
      assert.strictEqual(lines[2].indexOf('hits=1') !== -1, true);
      assert.strictEqual(lines[2].indexOf('writes=1') !== -1, true);
      assert.strictEqual(lines[2].indexOf('hot=0') !== -1, true);
    });
    it('respawns a fresh journal per audit pass', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      intl.getDetailed('value');
      intl.audit();
      const second: string[] = intl.audit();
      assert.strictEqual(second.length, 3);
    });
  });

  describe('locale market formatting', function() {
    it('formats plain numbers for the active language', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      assert.strictEqual(
        intl.formatNumber(1234.5),
        new Intl.NumberFormat('zh-CN').format(1234.5)
      );
    });
    it('honours caller overrides for number formatting', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      assert.strictEqual(
        intl.formatNumber(1234.5, { minimumFractionDigits: 3 }),
        new Intl.NumberFormat('zh-CN', { minimumFractionDigits: 3 }).format(
          1234.5
        )
      );
    });
    it('renders currency in the pack market style', function() {
      const zh: any = IntlFormat.init('zh-CN', METAS);
      assert.strictEqual(
        zh.formatCurrency(28.5),
        new Intl.NumberFormat('zh-CN', {
          style: 'currency',
          currency: 'CNY',
          useGrouping: true,
          minimumFractionDigits: 2,
          maximumFractionDigits: 2
        }).format(28.5)
      );
      const en: any = IntlFormat.init('en-US', METAS, 'zh-CN');
      assert.strictEqual(
        en.formatCurrency(28.5),
        new Intl.NumberFormat('en-US', {
          style: 'currency',
          currency: 'USD',
          useGrouping: true,
          minimumFractionDigits: 2,
          maximumFractionDigits: 2
        }).format(28.5)
      );
    });
    it('formats dates for the active language', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      const d = new Date(2020, 0, 2);
      assert.strictEqual(
        intl.formatDate(d),
        new Intl.DateTimeFormat('zh-CN').format(d)
      );
    });
  });

  describe('hot keys & pruning', function() {
    it('lists promoted keys only', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      intl.getDetailed('list-only-once');
      for (let i = 0; i < 3; i++) {
        intl.getDetailed('value');
      }
      const hot: string[] = intl.hotKeys(5);
      assert.strictEqual(hot.indexOf('value') !== -1, true);
      assert.strictEqual(hot.indexOf('list-only-once'), -1);
    });
    it('caps the hot key list to the requested limit', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      for (let i = 0; i < 3; i++) {
        intl.getDetailed('value');
        intl.getDetailed('test');
      }
      assert.strictEqual(intl.hotKeys(1).length, 1);
      assert.strictEqual(intl.hotKeys(5).length, 2);
    });
    it('prunes cold uncached entries and keeps cached ones', function() {
      const intl: any = IntlFormat.init('zh-CN', METAS);
      intl.describe('sweep-me');
      intl.getDetailed('test');
      assert.strictEqual(intl.pruneTranslations(), 1);
      assert.strictEqual(intl.pruneTranslations(), 0);
      assert.strictEqual(intl.getDetailed('test').entry.cacheWrites >= 1, true);
      const fresh: any = intl.getDetailed('sweep-me');
      assert.strictEqual(fresh.entry.hitCount, 1);
    });
  });

  describe('MessageEntry', function() {
    it('escalates hotness with hits and cache writes', function() {
      const entry = new MessageEntry('k', 'zh-CN');
      entry.touch(1);
      entry.touch(2);
      assert.strictEqual(entry.hotnessLevel, 1);
      entry.touch(3);
      assert.strictEqual(entry.hotnessLevel, 1);
      entry.cacheWrites = 1;
      entry.touch(4);
      assert.strictEqual(entry.hotnessLevel, 2);
    });
    it('matches keys by key or path', function() {
      const entry = new MessageEntry('k', 'zh-CN', 'a.b.k');
      assert.strictEqual(entry.matchesKey('k'), true);
      assert.strictEqual(entry.matchesKey('a.b.k'), true);
      assert.strictEqual(entry.matchesKey('nope'), false);
    });
    it('tracks content presence', function() {
      const entry = new MessageEntry('k', 'zh-CN');
      assert.strictEqual(entry.hasContent(), false);
      entry.content = 'text';
      assert.strictEqual(entry.hasContent(), true);
      entry.content = '';
      assert.strictEqual(entry.hasContent(), false);
    });
    it('records render timestamps and language', function() {
      const entry = new MessageEntry('k', 'zh-CN');
      entry.noteRender(100);
      entry.noteRender(200, 'en-US');
      assert.strictEqual(entry.renderCount, 2);
      assert.strictEqual(entry.lastRenderedAt, 200);
      assert.strictEqual(entry.lastRenderedLang, 'en-US');
    });
  });

  describe('LanguagePack', function() {
    it('derives market currency from the language', function() {
      assert.strictEqual(new LanguagePack('zh-CN', {}).currency, 'CNY');
      assert.strictEqual(new LanguagePack('en-US', {}).currency, 'USD');
      assert.strictEqual(new LanguagePack('en-US', {}).grouping, true);
      assert.strictEqual(new LanguagePack('en-US', {}).minimumFractionDigits, 2);
      assert.strictEqual(new LanguagePack('en-US', {}).maximumFractionDigits, 2);
    });
    it('records misses without duplicating recent keys', function() {
      const pack = new LanguagePack('zh-CN', {});
      pack.noteMiss('b');
      pack.noteMiss('a');
      pack.noteMiss('b');
      assert.strictEqual(pack.misses, 3);
      assert.deepStrictEqual(pack.recentKeys, ['b', 'a']);
    });
    it('caps the recent-key window and keeps misses unique', function() {
      const pack = new LanguagePack('zh-CN', {});
      for (let i = 0; i < 16; i++) {
        pack.noteMiss(`k${i}`);
      }
      assert.strictEqual(pack.recentKeys.length, 16);
      pack.noteMiss('k9');
      assert.strictEqual(pack.misses, 17);
      assert.strictEqual(pack.recentKeys.length, 16);
      assert.strictEqual(pack.recentKeys[0], 'k0');
      pack.noteMiss('extra');
      assert.strictEqual(pack.recentKeys.length, 16);
      assert.strictEqual(pack.recentKeys[15], 'extra');
      assert.strictEqual(pack.recentKeys[0], 'k1');
    });
    it('moves promoted keys to the front of the window', function() {
      const pack = new LanguagePack('zh-CN', {});
      pack.noteMiss('b');
      pack.noteMiss('a');
      pack.noteMiss('c');
      pack.notePromotion('a');
      assert.deepStrictEqual(pack.recentKeys, ['a', 'b', 'c']);
      pack.notePromotion('not-there');
      assert.strictEqual(pack.recentKeys[0], 'not-there');
    });
    it('resolves raw values and reports presence', function() {
      const pack = new LanguagePack('zh-CN', { a: { b: 'v' } });
      assert.strictEqual(pack.raw('a.b'), 'v');
      assert.strictEqual(pack.has('a.b'), true);
      assert.strictEqual(pack.has('missing'), false);
    });
    it('promotes entries that cross the hotness bar', function() {
      const pack = new LanguagePack('zh-CN', {});
      const cold = new MessageEntry('cold', 'zh-CN');
      assert.strictEqual(pack.shouldPromote(cold), false);
      const hot = new MessageEntry('hot', 'zh-CN');
      hot.hitCount = 4;
      hot.cacheWrites = 2;
      hot.hotnessLevel = 2;
      assert.strictEqual(pack.shouldPromote(hot), true);
    });
  });

  describe('TranslationCache', function() {
    it('counts hits and misses on recall', function() {
      const cache = new TranslationCache();
      cache.remember('k', 'v');
      assert.strictEqual(cache.recall('k'), 'v');
      assert.strictEqual(cache.recall('nope'), '');
      assert.strictEqual(cache.hits, 1);
      assert.strictEqual(cache.misses, 1);
    });
    it('evicts the oldest slot past capacity', function() {
      const cache = new TranslationCache(2);
      cache.remember('a', '1');
      cache.remember('b', '2');
      cache.remember('c', '3');
      assert.strictEqual(cache.evictions, 1);
      assert.strictEqual(cache.lastEvictedKey, 'a');
      assert.strictEqual(cache.slots.size, 2);
      assert.strictEqual(cache.recall('a'), '');
    });
    it('stamps hot entries into a marked slot and bumps cache writes', function() {
      const cache = new TranslationCache();
      const entry = new MessageEntry('k', 'zh-CN');
      entry.hotnessLevel = 2;
      const marked = cache.rememberEntry(entry, 'v');
      assert.strictEqual(marked, 'hot/zh-CN#k');
      assert.strictEqual(cache.slots.has('hot/zh-CN#k'), true);
      assert.strictEqual(entry.cacheWrites, 1);
    });
    it('resets the miss counter when a used uncached entry lands in the cache', function() {
      const cache = new TranslationCache();
      cache.recall('ghost');
      cache.recall('ghost');
      const entry = new MessageEntry('k', 'zh-CN');
      entry.lastUsedAt = 42;
      const marked = cache.rememberEntry(entry, 'v');
      assert.strictEqual(marked, 'zh-CN#k');
      assert.strictEqual(cache.misses, 0);
    });
    it('keeps hot or cached entries alive during eviction checks', function() {
      const cache = new TranslationCache();
      const hot = new MessageEntry('hot', 'zh-CN');
      hot.hitCount = 5;
      hot.hotnessLevel = 2;
      assert.strictEqual(cache.evictEntry(hot), false);
      const cached = new MessageEntry('cached', 'zh-CN');
      cache.rememberEntry(cached, 'v');
      assert.strictEqual(cache.evictEntry(cached), false);
      const cold = new MessageEntry('cold', 'zh-CN');
      cold.touch(9);
      assert.strictEqual(cache.evictEntry(cold), true);
    });
  });

  describe('IntlFormatter', function() {
    it('merges caller options and prunes unsupported ones', function() {
      const formatter = new IntlFormatter('zh-CN');
      const options = formatter.applyDefaults({
        style: 'decimal',
        minimumFractionDigits: 5
      });
      assert.strictEqual(options.style, 'decimal');
      assert.strictEqual(options.minimumFractionDigits, 5);
      assert.strictEqual('currency' in options, false);
      assert.strictEqual('useGrouping' in options, false);
      assert.deepStrictEqual(formatter.applyDefaults(undefined), {});
    });
    it('formats numbers with the configured default locale', function() {
      const formatter = new IntlFormatter('fr-FR');
      assert.strictEqual(
        formatter.formatNumber(1234.5),
        new Intl.NumberFormat('fr-FR').format(1234.5)
      );
    });
    it('renders entry content and settles the render language once', function() {
      const formatter = new IntlFormatter('zh-CN');
      const entry = new MessageEntry('k', 'zh-CN');
      entry.content = '好的';
      assert.strictEqual(formatter.renderEntry(entry), '好的');
      assert.strictEqual(entry.renderCount, 1);
      assert.strictEqual(entry.lastRenderedLang, 'zh-CN');
      assert.strictEqual(formatter.renderEntry(entry), '好的');
      assert.strictEqual(entry.renderCount, 1);
    });
    it('journals unusable patterns instead of throwing', function() {
      const formatter = new IntlFormatter('zh-CN');
      const entry = new MessageEntry('bad', 'zh-CN');
      entry.content = 'oops{';
      assert.strictEqual(formatter.renderEntry(entry), '');
      assert.deepStrictEqual(formatter.failures, ['oops{', 'bad']);
      assert.strictEqual(entry.renderCount, 0);
      const empty = new MessageEntry('empty', 'zh-CN');
      assert.strictEqual(formatter.renderEntry(empty), '');
      assert.strictEqual(formatter.failures.length, 2);
      assert.strictEqual(formatter.failures[1], 'bad');
    });
  });

  describe('IntlAudit', function() {
    it('stays silent when disabled', function() {
      const audit = new IntlAudit(false);
      const entry = new MessageEntry('k', 'zh-CN');
      assert.strictEqual(audit.recordEntryUsage(entry), false);
      assert.strictEqual(audit.attachInstance({ __lang__: 'zh-CN' }), null);
      assert.deepStrictEqual(audit.lines(), []);
    });
    it('journals usage lines and instance snapshots', function() {
      const audit = new IntlAudit(true);
      const entry = new MessageEntry('k', 'zh-CN');
      entry.touch(7);
      audit.begin();
      assert.strictEqual(audit.recordEntryUsage(entry), true);
      assert.strictEqual(audit.size(), 1);
      assert.strictEqual(audit.lines()[0].indexOf('resolve k@zh-CN'), 0);
      assert.strictEqual(audit.lines()[0].indexOf('hits=1') !== -1, true);
      audit.attachInstance({
        __lang__: 'zh-CN',
        __metas__: METAS,
        __data__: METAS['zh-CN'],
        __defaultKey__: 'zh-CN',
        __entries__: { size: 0 },
        __cache__: { hits: 4 },
        auditSnapshot() {
          return { lang: 'zh-CN', entries: 0, cacheHits: 4 };
        }
      });
      assert.deepStrictEqual(audit.lines().slice(0, 2), [
        'instance lang=zh-CN',
        'instance entries=0 cacheHits=4'
      ]);
    });
  });
});
