import { CacheRegistry, Config, Tuple } from './cache'
import { query } from './query'
import { shallowEqualArrays } from './utils'

const registry = new CacheRegistry()

const suspend = <Keys extends Tuple<unknown>, Fn extends (...keys: Keys) => Promise<unknown>>(
  fn: Fn | Promise<unknown>,
  keys?: Keys,
  config?: Config
) => query(registry, fn, keys, false, config)

const preload = <Keys extends Tuple<unknown>, Fn extends (...keys: Keys) => Promise<unknown>>(
  fn: Fn | Promise<unknown>,
  keys?: Keys,
  config?: Config
) => void query(registry, fn, keys, true, config)

const peek = <Keys extends Tuple<unknown>>(keys: Keys) =>
  registry.entries.find((entry) => shallowEqualArrays(keys, entry.keys, entry.equal))?.response

const clear = <Keys extends Tuple<unknown>>(keys?: Keys) => {
  if (keys === undefined || keys.length === 0) registry.entries.splice(0, registry.entries.length)
  else {
    for (const entry of registry.entries) {
      if (shallowEqualArrays(keys, entry.keys, entry.equal)) {
        const index = registry.entries.indexOf(entry)
        if (index !== -1) registry.entries.splice(index, 1)
        return
      }
    }
  }
}

export { suspend, clear, preload, peek }
