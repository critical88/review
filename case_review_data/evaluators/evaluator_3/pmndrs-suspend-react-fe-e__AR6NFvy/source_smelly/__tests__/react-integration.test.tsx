import React, { ReactNode, Suspense } from 'react'
import TestRenderer, { act } from 'react-test-renderer'
import { suspend, preload, peek, clear } from '../src'

const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve()
  await new Promise((resolve) => setTimeout(resolve, 0))
  for (let i = 0; i < 10; i++) await Promise.resolve()
}

const settle = async () => {
  await act(async () => {
    await flush()
  })
  await act(async () => {
    await flush()
  })
}

let renderer: TestRenderer.ReactTestRenderer | undefined

const render = (node: ReactNode) => {
  act(() => {
    renderer = TestRenderer.create(node)
  })
}

beforeEach(() => {
  clear()
})

afterEach(() => {
  if (renderer) {
    act(() => {
      renderer!.unmount()
    })
    renderer = undefined
  }
  clear()
})

class Boundary extends React.Component<unknown, { error: unknown }> {
  state = { error: undefined as unknown }

  static getDerivedStateFromError(error: unknown) {
    return { error }
  }

  render() {
    if (this.state.error) return <b>error</b>
    return this.props.children as ReactNode
  }
}

describe('react suspense integration', () => {
  test('renders the fallback while the request is pending', () => {
    const fn = () => Promise.resolve('data')
    const Component = () => <span>{suspend(fn)}</span>
    render(
      <Suspense fallback={<em>loading</em>}>
        <Component />
      </Suspense>
    )
    expect(renderer!.root.findByType('em').children[0]).toBe('loading')
    expect(renderer!.root.findAllByType('span').length).toBe(0)
  })

  test('renders the resolved value once the request settles', async () => {
    const fn = () => Promise.resolve('data')
    const Component = () => <span>{suspend(fn)}</span>
    render(
      <Suspense fallback={<em>loading</em>}>
        <Component />
      </Suspense>
    )
    await settle()
    expect(renderer!.root.findByType('span').children[0]).toBe('data')
  })

  test('bubbles captured rejections into an error boundary', async () => {
    const boom = new Error('render-boom')
    const fn = () => Promise.reject(boom)
    const Component = () => <span>{suspend(fn)}</span>
    render(
      <Boundary>
        <Suspense fallback={<em>loading</em>}>
          <Component />
        </Suspense>
      </Boundary>
    )
    await settle()
    expect(renderer!.root.findByType('b')).toBeTruthy()
    expect(renderer!.root.findAllByType('span').length).toBe(0)
  })

  test('shares a single cache entry between sibling components', async () => {
    const fn = jest.fn(() => Promise.resolve('shared'))
    const Component = () => <span>{suspend(fn, ['shared'])}</span>
    render(
      <Suspense fallback={<em>loading</em>}>
        <Component />
        <Component />
      </Suspense>
    )
    await settle()
    const spans = renderer!.root.findAllByType('span')
    expect(spans.length).toBe(2)
    spans.forEach((span) => expect(span.children[0]).toBe('shared'))
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('a preloaded entry is returned without a fallback render', async () => {
    const fn = jest.fn(() => Promise.resolve('warmed'))
    await act(async () => {
      preload(fn, ['warm'])
      await flush()
    })
    const Component = () => <span>{suspend(fn, ['warm'])}</span>
    render(
      <Suspense fallback={<em>loading</em>}>
        <Component />
      </Suspense>
    )
    expect(renderer!.root.findByType('span').children[0]).toBe('warmed')
    expect(renderer!.root.findAllByType('em').length).toBe(0)
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('a remounted component reuses the cached response', async () => {
    const fn = jest.fn(() => Promise.resolve('cached'))
    const Component = () => <span>{suspend(fn, ['cached'])}</span>
    render(
      <Suspense fallback={<em>loading</em>}>
        <Component />
      </Suspense>
    )
    await settle()
    expect(fn).toHaveBeenCalledTimes(1)
    act(() => {
      renderer!.unmount()
    })
    render(
      <Suspense fallback={<em>loading</em>}>
        <Component />
      </Suspense>
    )
    expect(renderer!.root.findByType('span').children[0]).toBe('cached')
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('new keys trigger another request and a fresh response', async () => {
    const fn = jest.fn((id: string) => Promise.resolve(`result-${id}`))
    const Component = ({ id }: { id: string }) => <span>{suspend(fn, [id])}</span>
    render(
      <Suspense fallback={<em>loading</em>}>
        <Component id="a" />
      </Suspense>
    )
    await settle()
    expect(renderer!.root.findByType('span').children[0]).toBe('result-a')
    await act(async () => {
      renderer!.update(
        <Suspense fallback={<em>loading</em>}>
          <Component id="b" />
        </Suspense>
      )
      await flush()
    })
    await settle()
    expect(renderer!.root.findByType('span').children[0]).toBe('result-b')
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('unmounting during an in-flight request is safe', async () => {
    let resolve: (value: unknown) => void
    const fn = () =>
      new Promise<unknown>((res) => {
        resolve = res
      })
    const Component = () => <span>{suspend(fn, ['late'])}</span>
    render(
      <Suspense fallback={<em>loading</em>}>
        <Component />
      </Suspense>
    )
    act(() => {
      renderer!.unmount()
    })
    await act(async () => {
      resolve!('late')
      await flush()
    })
    expect(peek(['late'])).toBe('late')
  })
})
