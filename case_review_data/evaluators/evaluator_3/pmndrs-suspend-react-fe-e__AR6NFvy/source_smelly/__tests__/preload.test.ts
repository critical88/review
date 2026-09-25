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

describe('preload behavior', () => {
  test('registers a new entry without throwing', () => {
    const fn = jest.fn(() => Promise.resolve('warm'))
    expect(() => preload(fn)).not.toThrow()
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('returns undefined when registering a new entry', () => {
    const fn = () => Promise.resolve('warm')
    expect(preload(fn)).toBeUndefined()
  })

  test('returns undefined for an already known entry', () => {
    const fn = jest.fn(() => new Promise<unknown>(() => {}))
    preload(fn)
    expect(preload(fn)).toBeUndefined()
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('exposes the response through peek once resolved', async () => {
    const fn = () => Promise.resolve('warm')
    preload(fn)
    await flush()
    expect(peek([fn] as [unknown])).toBe('warm')
  })

  test('peek is undefined while the preloaded request is in flight', async () => {
    const fn = () => new Promise<unknown>(() => {})
    preload(fn)
    expect(peek([fn] as [unknown])).toBeUndefined()
    await flush()
  })

  test('executes the function once for repeated preloads', () => {
    const fn = jest.fn(() => Promise.resolve('warm'))
    preload(fn, ['once'])
    preload(fn, ['once'])
    preload(fn, ['once'])
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('a later suspend reuses the preloaded entry', async () => {
    const fn = jest.fn(() => Promise.resolve('warm'))
    preload(fn, ['shared'])
    const thrown = captureThrown(() => suspend(fn, ['shared']))
    expect(thrown).not.toBeUndefined()
    expect(fn).toHaveBeenCalledTimes(1)
    await flush()
    expect(suspend(fn, ['shared'])).toBe('warm')
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('passes keys through to the preloaded function', () => {
    const fn = jest.fn((a: string) => Promise.resolve(a.toUpperCase()))
    preload(fn, ['input'])
    expect(fn).toHaveBeenCalledWith('input')
  })

  test('preload of a promise value registers the promise', async () => {
    const promise = Promise.resolve('direct-preload')
    preload(promise, ['promise-key'])
    expect(peek(['promise-key'])).toBeUndefined()
    await flush()
    expect(peek(['promise-key'])).toBe('direct-preload')
  })

  test('does not refresh the removal timeout of an existing entry', async () => {
    jest.useFakeTimers()
    try {
      const fn = jest.fn(() => Promise.resolve('ttl'))
      preload(fn, ['ttl'], { lifespan: 100 })
      await flush()
      jest.advanceTimersByTime(50)
      preload(fn, ['ttl'], { lifespan: 100 })
      jest.advanceTimersByTime(49)
      expect(peek(['ttl'])).toBe('ttl')
      jest.advanceTimersByTime(1)
      expect(peek(['ttl'])).toBeUndefined()
    } finally {
      jest.useRealTimers()
    }
  })
})
