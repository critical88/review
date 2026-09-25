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

describe('clear behavior', () => {
  test('removes every entry when called without keys', async () => {
    const fn = () => Promise.resolve('value')
    captureThrown(() => suspend(fn, ['one']))
    captureThrown(() => suspend(fn, ['two']))
    await flush()
    clear()
    expect(peek(['one'])).toBeUndefined()
    expect(peek(['two'])).toBeUndefined()
  })

  test('treats an empty key array as a request to clear everything', async () => {
    const fn = () => Promise.resolve('value')
    captureThrown(() => suspend(fn, ['one']))
    captureThrown(() => suspend(fn, ['two']))
    await flush()
    clear([] as [])
    expect(peek(['one'])).toBeUndefined()
    expect(peek(['two'])).toBeUndefined()
  })

  test('removes only the entry matching the given keys', async () => {
    const fn = () => Promise.resolve('value')
    captureThrown(() => suspend(fn, ['keep']))
    captureThrown(() => suspend(fn, ['drop']))
    await flush()
    clear(['drop'])
    expect(peek(['keep'])).toBe('value')
    expect(peek(['drop'])).toBeUndefined()
  })

  test('is a no-op for keys without an entry', async () => {
    const fn = () => Promise.resolve('value')
    captureThrown(() => suspend(fn, ['kept']))
    await flush()
    expect(() => clear(['never-registered'])).not.toThrow()
    expect(peek(['kept'])).toBe('value')
  })

  test('forces a fresh execution on the next read', async () => {
    const fn = jest.fn(() => Promise.resolve('fresh'))
    captureThrown(() => suspend(fn, ['regen']))
    await flush()
    expect(suspend(fn, ['regen'])).toBe('fresh')
    clear(['regen'])
    captureThrown(() => suspend(fn, ['regen']))
    await flush()
    expect(suspend(fn, ['regen'])).toBe('fresh')
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('uses the entry equality function to find the entry', async () => {
    const fn = () => Promise.resolve('custom')
    const equal = (a: any, b: any) => a.id === b.id
    captureThrown(() => suspend(fn, [{ id: 7 }], { equal }))
    await flush()
    clear([{ id: 7 }] as [{ id: number }])
    expect(peek([{ id: 7 }])).toBeUndefined()
  })

  test('removes an entry that is still in flight', async () => {
    const fn = jest.fn(() => new Promise<unknown>(() => {}))
    captureThrown(() => suspend(fn, ['in-flight']))
    clear(['in-flight'])
    expect(peek(['in-flight'])).toBeUndefined()
    captureThrown(() => suspend(fn, ['in-flight']))
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('allows the cache to be reused after a full clear', async () => {
    const fn = jest.fn(() => Promise.resolve('reused'))
    captureThrown(() => suspend(fn, ['cycle']))
    await flush()
    clear()
    captureThrown(() => suspend(fn, ['cycle']))
    await flush()
    expect(suspend(fn, ['cycle'])).toBe('reused')
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('clearing a preloaded entry cancels its visibility', async () => {
    preload(() => Promise.resolve('gone'), ['preloaded'])
    await flush()
    clear(['preloaded'])
    expect(peek(['preloaded'])).toBeUndefined()
  })

  test('matches cleared keys by content rather than identity', async () => {
    const fn = () => Promise.resolve('content')
    captureThrown(() => suspend(fn, ['content-clear']))
    await flush()
    clear(['content-clear'])
    expect(peek(['content-clear'])).toBeUndefined()
  })
})
