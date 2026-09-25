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

describe('error handling', () => {
  test('rethrows the captured rejection on the next read', async () => {
    const boom = new Error('boom')
    let reject: (reason?: unknown) => void
    const fn = () =>
      new Promise<unknown>((_, rej) => {
        reject = rej
      })
    captureThrown(() => suspend(fn, ['failing']))
    reject!(boom)
    await flush()
    const thrown = captureThrown(() => suspend(fn, ['failing']))
    expect(thrown).toBe(boom)
  })

  test('keeps throwing the promise while the request is in flight', () => {
    let reject: (reason?: unknown) => void
    const fn = () =>
      new Promise<unknown>((_, rej) => {
        reject = rej
      })
    const first = captureThrown(() => suspend(fn, ['pending']))
    const second = captureThrown(() => suspend(fn, ['pending']))
    expect(first).toBe(second)
    reject!(new Error('later'))
  })

  test('stores rejections once and reports them on every read', async () => {
    const boom = new Error('boom')
    const fn = () => Promise.reject(boom)
    captureThrown(() => suspend(fn))
    await flush()
    expect(captureThrown(() => suspend(fn))).toBe(boom)
    expect(captureThrown(() => suspend(fn))).toBe(boom)
  })

  test('a rejected entry exposes no response', async () => {
    const fn = () => Promise.reject(new Error('boom'))
    captureThrown(() => suspend(fn, ['keyed-error']))
    await flush()
    expect(peek(['keyed-error'])).toBeUndefined()
  })

  test('rethrows non-error rejection values unchanged', async () => {
    const fn = () => Promise.reject('plain-string')
    captureThrown(() => suspend(fn, ['string-error']))
    await flush()
    const thrown = captureThrown(() => suspend(fn, ['string-error']))
    expect(thrown).toBe('plain-string')
  })

  test('captures rejections of directly supplied promises', async () => {
    const boom = new Error('direct-boom')
    const promise = Promise.reject(boom)
    // Swallow the unhandled rejection like a Suspense boundary would.
    promise.catch(() => {})
    captureThrown(() => suspend(promise, ['direct-error']))
    await flush()
    expect(captureThrown(() => suspend(promise, ['direct-error']))).toBe(boom)
  })

  test('synchronous throws inside the function are not cached', () => {
    const fn = () => {
      throw new Error('sync')
    }
    expect(() => suspend(fn, ['sync'])).toThrow('sync')
    expect(() => suspend(fn, ['sync'])).toThrow('sync')
    expect(peek(['sync'])).toBeUndefined()
  })

  test('a cleared error entry can succeed again', async () => {
    let shouldFail = true
    const fn = () =>
      shouldFail ? Promise.reject(new Error('first attempt')) : Promise.resolve('second attempt')
    captureThrown(() => suspend(fn, ['retry']))
    await flush()
    expect(captureThrown(() => suspend(fn, ['retry']))).toEqual(new Error('first attempt'))
    clear(['retry'])
    shouldFail = false
    captureThrown(() => suspend(fn, ['retry']))
    await flush()
    expect(suspend(fn, ['retry'])).toBe('second attempt')
  })

  test('preloaded rejections surface through suspend', async () => {
    const boom = new Error('preloaded-boom')
    preload(() => Promise.reject(boom), ['preloaded-error'])
    await flush()
    expect(captureThrown(() => suspend(() => Promise.resolve('unused'), ['preloaded-error']))).toBe(boom)
  })

  test('errors stop the inclusion of the entry in lifespan scheduling', async () => {
    jest.useFakeTimers()
    try {
      const fn = () => Promise.reject(new Error('expired-error'))
      captureThrown(() => suspend(fn, ['ttl-error'], { lifespan: 100 }))
      await flush()
      jest.advanceTimersByTime(1000)
      expect(captureThrown(() => suspend(fn, ['ttl-error'], { lifespan: 100 }))).toEqual(
        new Error('expired-error')
      )
    } finally {
      jest.useRealTimers()
    }
  })
})
