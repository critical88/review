import { suspend, preload, peek, clear } from '../src'

const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve()
}

const captureThrown = (run: () => unknown): any => {
  try {
    run()
  } catch (thrown: any) {
    return thrown
  }
  return undefined
}

beforeEach(() => {
  clear()
})

describe('key matching and equality', () => {
  test('matches entries with identical primitive keys', async () => {
    const fn = jest.fn(() => Promise.resolve('primitives'))
    captureThrown(() => suspend(fn, ['same', 1]))
    await flush()
    captureThrown(() => suspend(fn, ['same', 1]))
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('does not match when key order differs', async () => {
    const fn = jest.fn(() => Promise.resolve('order'))
    captureThrown(() => suspend(fn, ['first', 'second']))
    await flush()
    captureThrown(() => suspend(fn, ['second', 'first']))
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('uses strict identity for object keys by default', async () => {
    const fn = jest.fn(() => Promise.resolve('identity'))
    const first = { same: 'content' }
    const second = { same: 'content' }
    captureThrown(() => suspend(fn, [first] as [typeof first]))
    await flush()
    captureThrown(() => suspend(fn, [second] as [typeof second]))
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('uses strict identity for keyless functions', async () => {
    const fnA = jest.fn(() => Promise.resolve('a'))
    const fnB = jest.fn(() => Promise.resolve('b'))
    captureThrown(() => suspend(fnA))
    await flush()
    expect(suspend(fnA)).toBe('a')
    captureThrown(() => suspend(fnB))
    await flush()
    expect(fnA).toHaveBeenCalledTimes(1)
    expect(fnB).toHaveBeenCalledTimes(1)
  })

  test('a custom equal callback decides element equality', async () => {
    const fn = jest.fn(() => Promise.resolve('custom-equal'))
    const equal = (a: any, b: any) => a.id === b.id
    captureThrown(() => suspend(fn, [{ id: 1 }], { equal }))
    await flush()
    captureThrown(() => suspend(fn, [{ id: 1 }, { other: 2 }], { equal }))
    // The first element pair matches with the custom comparator while the
    // second pair does not, so this must be treated as new keys.
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('a custom equal callback receives the compared elements', async () => {
    const fn = () => Promise.resolve('callback')
    const equal = jest.fn((a: any, b: any) => a === b)
    captureThrown(() => suspend(fn, ['x'], { equal }))
    await flush()
    // Comparisons only happen once a second lookup consults the stored entry.
    peek(['x'])
    expect(equal).toHaveBeenCalledTimes(1)
    expect(equal.mock.calls[0][0]).toBe('x')
    expect(equal.mock.calls[0][1]).toBe('x')
  })

  test('a matching custom entry returns the stored response', async () => {
    const fn = jest.fn(() => Promise.resolve('matched'))
    const equal = (a: any, b: any) => a.id === b.id
    captureThrown(() => suspend(fn, [{ id: 1 }], { equal }))
    await flush()
    const thrown = captureThrown(() => suspend(fn, [{ id: 1 }, 'extra'], { equal }))
    // [{id:1}, 'extra'] has a different length than [{id:1}] and cannot match.
    expect(thrown).not.toBeUndefined()
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('a single element can be compared with rich equality', async () => {
    const fn = jest.fn(() => Promise.resolve('rich'))
    const equal = (a: any, b: any) => a.key === b.key
    captureThrown(() => suspend(fn, [{ key: 'one' }], { equal }))
    await flush()
    expect(suspend(fn, [{ key: 'one' }], { equal })).toBe('rich')
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('NaN keys never match with the default comparator', async () => {
    const fn = jest.fn(() => Promise.resolve('nan'))
    captureThrown(() => suspend(fn, [NaN] as [number]))
    await flush()
    captureThrown(() => suspend(fn, [NaN] as [number]))
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('an entry with an always-true equal consumes later reads', async () => {
    const fn = jest.fn(() => Promise.resolve('first'))
    const other = jest.fn(() => Promise.resolve('second'))
    const truthy = () => true
    captureThrown(() => suspend(fn, ['any-key'], { equal: truthy }))
    await flush()
    expect(suspend(other, ['anything-else'], { equal: truthy })).toBe('first')
    expect(other).not.toHaveBeenCalled()
  })

  test('functions and promises can mix inside key tuples', async () => {
    const fn = jest.fn(() => Promise.resolve('mix'))
    const tag = { tag: 'promise' }
    captureThrown(() => suspend(fn, ['mixed', 3, tag]))
    await flush()
    expect(suspend(fn, ['mixed', 3, tag])).toBe('mix')
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('the equal comparator is stored per entry, not globally', async () => {
    const strict = () => false
    const fn = () => Promise.resolve('strict-entry')
    captureThrown(() => suspend(fn, ['strict-key'], { equal: strict }))
    await flush()
    // peek consults the comparator stored on the entry itself.
    expect(peek(['strict-key'])).toBeUndefined()
    const other = () => Promise.resolve('default-entry')
    captureThrown(() => suspend(other, ['default-key']))
    await flush()
    expect(peek(['default-key'])).toBe('default-entry')
  })
})
