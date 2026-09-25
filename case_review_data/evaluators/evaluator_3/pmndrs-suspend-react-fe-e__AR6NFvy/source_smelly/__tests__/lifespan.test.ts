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
  jest.useFakeTimers()
})

afterEach(() => {
  jest.useRealTimers()
})

describe('lifespan handling', () => {
  test('removes a resolved entry once the lifespan elapses', async () => {
    const fn = () => Promise.resolve('temporary')
    captureThrown(() => suspend(fn, ['ttl'], { lifespan: 100 }))
    await flush()
    expect(peek(['ttl'])).toBe('temporary')
    jest.advanceTimersByTime(100)
    expect(peek(['ttl'])).toBeUndefined()
  })

  test('keeps the entry alive before the lifespan elapses', async () => {
    const fn = () => Promise.resolve('temporary')
    captureThrown(() => suspend(fn, ['ttl'], { lifespan: 1000 }))
    await flush()
    jest.advanceTimersByTime(999)
    expect(peek(['ttl'])).toBe('temporary')
    jest.advanceTimersByTime(1)
    expect(peek(['ttl'])).toBeUndefined()
  })

  test('re-executes the function after the entry expires', async () => {
    const fn = jest.fn(() => Promise.resolve('regenerated'))
    captureThrown(() => suspend(fn, ['expired'], { lifespan: 100 }))
    await flush()
    expect(suspend(fn, ['expired'], { lifespan: 100 })).toBe('regenerated')
    jest.advanceTimersByTime(100)
    captureThrown(() => suspend(fn, ['expired'], { lifespan: 100 }))
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('refreshes the removal timeout on every read', async () => {
    const fn = () => Promise.resolve('refreshed')
    captureThrown(() => suspend(fn, ['ttl'], { lifespan: 100 }))
    await flush()
    jest.advanceTimersByTime(75)
    suspend(fn, ['ttl'], { lifespan: 100 })
    jest.advanceTimersByTime(75)
    // The original timer was cancelled and replaced by a fresh window.
    expect(peek(['ttl'])).toBe('refreshed')
    jest.advanceTimersByTime(25)
    expect(peek(['ttl'])).toBeUndefined()
  })

  test('uses the lifespan of the most recent read', async () => {
    const fn = () => Promise.resolve('extended')
    captureThrown(() => suspend(fn, ['ttl'], { lifespan: 50 }))
    await flush()
    jest.advanceTimersByTime(30)
    suspend(fn, ['ttl'], { lifespan: 500 })
    jest.advanceTimersByTime(50)
    // The short original window is gone; the extended window is still open.
    expect(peek(['ttl'])).toBe('extended')
    jest.advanceTimersByTime(500)
    expect(peek(['ttl'])).toBeUndefined()
  })

  test('does not schedule a timer while the request is in flight', async () => {
    const fn = () => new Promise<unknown>(() => {})
    captureThrown(() => suspend(fn, ['ttl'], { lifespan: 100 }))
    jest.advanceTimersByTime(500)
    // No timer should have fired because the promise never resolved.
  })

  test('ignores zero and negative lifespans', async () => {
    const fn = () => Promise.resolve('forever')
    captureThrown(() => suspend(fn, ['zero'], { lifespan: 0 }))
    captureThrown(() => suspend(fn, ['negative'], { lifespan: -100 }))
    captureThrown(() => suspend(fn, ['absent']))
    await flush()
    jest.advanceTimersByTime(100000)
    expect(peek(['zero'])).toBe('forever')
    expect(peek(['negative'])).toBe('forever')
    expect(peek(['absent'])).toBe('forever')
  })

  test('expires entries with independent lifespans separately', async () => {
    const fn = () => Promise.resolve('value')
    captureThrown(() => suspend(fn, ['short'], { lifespan: 100 }))
    captureThrown(() => suspend(fn, ['long'], { lifespan: 300 }))
    await flush()
    jest.advanceTimersByTime(100)
    expect(peek(['short'])).toBeUndefined()
    expect(peek(['long'])).toBe('value')
    jest.advanceTimersByTime(200)
    expect(peek(['long'])).toBeUndefined()
  })

  test('does not call the function again when refreshing the timeout', async () => {
    const fn = jest.fn(() => Promise.resolve('cool'))
    captureThrown(() => suspend(fn, ['ttl'], { lifespan: 100 }))
    await flush()
    suspend(fn, ['ttl'], { lifespan: 100 })
    suspend(fn, ['ttl'], { lifespan: 100 })
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('a preloaded entry expires on the same schedule', async () => {
    const fn = () => Promise.resolve('preloaded-ttl')
    preload(fn, ['ttl'], { lifespan: 100 })
    await flush()
    expect(peek(['ttl'])).toBe('preloaded-ttl')
    jest.advanceTimersByTime(100)
    expect(peek(['ttl'])).toBeUndefined()
  })

  test('clear cancels the pending lifespan timer without side effects', async () => {
    const fn = jest.fn(() => Promise.resolve('cleared-ttl'))
    captureThrown(() => suspend(fn, ['ttl'], { lifespan: 100 }))
    await flush()
    clear(['ttl'])
    jest.advanceTimersByTime(1000)
    expect(peek(['ttl'])).toBeUndefined()
  })
})
