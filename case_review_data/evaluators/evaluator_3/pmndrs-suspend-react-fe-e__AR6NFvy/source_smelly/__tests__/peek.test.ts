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

describe('peek behavior', () => {
  test('returns undefined for unknown keys', () => {
    expect(peek(['missing'])).toBeUndefined()
  })

  test('returns undefined while an entry is still in flight', () => {
    const fn = () => new Promise<unknown>(() => {})
    captureThrown(() => suspend(fn, ['pending']))
    expect(peek(['pending'])).toBeUndefined()
  })

  test('returns the response of a resolved entry', async () => {
    const fn = () => Promise.resolve('visible')
    captureThrown(() => suspend(fn, ['visible']))
    await flush()
    expect(peek(['visible'])).toBe('visible')
  })

  test('matches by key content, not array identity', async () => {
    const fn = () => Promise.resolve('content')
    captureThrown(() => suspend(fn, ['content-key']))
    await flush()
    expect(peek(['content-key'])).toBe('content')
  })

  test('does not match when key length differs', async () => {
    const fn = () => Promise.resolve('length')
    captureThrown(() => suspend(fn, ['one', 'two']))
    await flush()
    expect(peek(['one'])).toBeUndefined()
    expect(peek(['one', 'two', 'three'])).toBeUndefined()
  })

  test('uses the entry equality function for matching', async () => {
    const fn = () => Promise.resolve('custom')
    const equal = (a: any, b: any) => a.id === b.id
    captureThrown(() => suspend(fn, [{ id: 1 }], { equal }))
    await flush()
    expect(peek([{ id: 1 }])).toBe('custom')
    expect(peek([{ id: 2 }])).toBeUndefined()
  })

  test('finds a keyless entry through its function', async () => {
    const fn = () => Promise.resolve('keyless')
    captureThrown(() => suspend(fn))
    await flush()
    expect(peek([fn] as [unknown])).toBe('keyless')
  })

  test('does not register new entries', () => {
    const fn = jest.fn(() => Promise.resolve('no-side-effect'))
    expect(peek(['fresh'])).toBeUndefined()
    captureThrown(() => suspend(fn, ['fresh']))
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('returns undefined for entries without a response', async () => {
    const promise = Promise.resolve('stored')
    captureThrown(() => suspend(promise, ['promise-key']))
    // The entry exists under these keys, but only a promise is stored so far.
    expect(peek(['promise-key'])).toBeUndefined()
    await flush()
    expect(peek(['promise-key'])).toBe('stored')
  })

  test('tolerates being called without keys', () => {
    expect(() => peek(undefined as any)).not.toThrow()
    expect(peek(undefined as any)).toBeUndefined()
  })
})
