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

describe('suspend core behavior', () => {
  test('throws a thenable the first time a request is made', () => {
    const fn = () => Promise.resolve('value')
    const thrown = captureThrown(() => suspend(fn))
    expect(thrown).toBeDefined()
    expect(typeof (thrown as Promise<unknown>).then).toBe('function')
  })

  test('executes the function immediately when registering the entry', () => {
    const fn = jest.fn(() => Promise.resolve('value'))
    captureThrown(() => suspend(fn))
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('returns the response once the promise has resolved', async () => {
    const fn = () => Promise.resolve('value')
    captureThrown(() => suspend(fn))
    await flush()
    expect(suspend(fn)).toBe('value')
  })

  test('returns the exact resolved object identity', async () => {
    const data = { id: 1 }
    const fn = () => Promise.resolve(data)
    captureThrown(() => suspend(fn))
    await flush()
    expect(suspend(fn)).toBe(data)
  })

  test('consecutive reads of an in-flight entry throw the same promise instance', () => {
    const fn = jest.fn(() => new Promise<unknown>(() => {}))
    const first = captureThrown(() => suspend(fn))
    const second = captureThrown(() => suspend(fn))
    expect(first).toBe(second)
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('keeps throwing while the promise is unresolved', () => {
    const fn = () => new Promise<unknown>(() => {})
    for (let i = 0; i < 3; i++) {
      expect(() => suspend(fn)).toThrow()
    }
  })

  test('only executes the function once for identical keys', async () => {
    const fn = jest.fn(() => Promise.resolve(1))
    captureThrown(() => suspend(fn, ['key']))
    await flush()
    suspend(fn, ['key'])
    suspend(fn, ['key'])
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('passes the key elements as function arguments', async () => {
    const fn = jest.fn((a: number, b: number) => Promise.resolve(a + b))
    captureThrown(() => suspend(fn, [2, 3]))
    await flush()
    expect(fn).toHaveBeenCalledWith(2, 3)
    expect(suspend(fn, [2, 3])).toBe(5)
  })

  test('invokes a keyless function with itself as the key argument', () => {
    const fn = jest.fn(() => Promise.resolve('x'))
    captureThrown(() => suspend(fn))
    expect(fn).toHaveBeenCalledTimes(1)
    expect(fn.mock.calls[0][0]).toBe(fn)
  })

  test('uses the function itself as the key when no keys are given', async () => {
    const fn = () => Promise.resolve('self-keyed')
    captureThrown(() => suspend(fn))
    await flush()
    expect(peek([fn] as [unknown])).toBe('self-keyed')
  })

  test('passes a resolved promise directly and returns its value on the next read', async () => {
    const promise = Promise.resolve('direct')
    const thrown = captureThrown(() => suspend(promise))
    expect(thrown).toBeDefined()
    await flush()
    expect(suspend(promise)).toBe('direct')
  })

  test('throws a derived thenable rather than the input promise', () => {
    const promise = Promise.resolve('direct')
    const thrown = captureThrown(() => suspend(promise))
    expect(thrown).not.toBe(promise)
    expect(typeof (thrown as Promise<unknown>).then).toBe('function')
  })

  test('registers a promise under explicit keys', async () => {
    const promise = Promise.resolve('keyed-direct')
    captureThrown(() => suspend(promise, ['promise-key']))
    await flush()
    expect(peek(['promise-key'])).toBe('keyed-direct')
  })

  test('treats an empty key array as a stable key', async () => {
    const fn = jest.fn(() => Promise.resolve('empty-keyed'))
    captureThrown(() => suspend(fn, []))
    await flush()
    expect(fn).toHaveBeenCalledTimes(1)
    expect(suspend(fn, [])).toBe('empty-keyed')
  })

  test('coexisting entries keep separate responses', async () => {
    const fnA = () => Promise.resolve('a')
    const fnB = () => Promise.resolve('b')
    captureThrown(() => suspend(fnA, ['a']))
    captureThrown(() => suspend(fnB, ['b']))
    await flush()
    expect(suspend(fnA, ['a'])).toBe('a')
    expect(suspend(fnB, ['b'])).toBe('b')
  })

  test('re-executes the function after its entry has been cleared', async () => {
    const fn = jest.fn(() => Promise.resolve('again'))
    captureThrown(() => suspend(fn, ['re']))
    await flush()
    expect(suspend(fn, ['re'])).toBe('again')
    expect(fn).toHaveBeenCalledTimes(1)
    clear(['re'])
    captureThrown(() => suspend(fn, ['re']))
    await flush()
    expect(suspend(fn, ['re'])).toBe('again')
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('keys alone determine the entry, not the function that first created it', async () => {
    const first = jest.fn(() => Promise.resolve('first'))
    const second = jest.fn(() => Promise.resolve('second'))
    captureThrown(() => suspend(first, ['shared']))
    await flush()
    expect(suspend(second, ['shared'])).toBe('first')
    expect(second).not.toHaveBeenCalled()
  })
})
