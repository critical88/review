import { Await, CacheEntry, CacheRegistry, Config, Tuple } from './cache'
import { isPromise, shallowEqualArrays } from './utils'

export function query<Keys extends Tuple<unknown>, Fn extends (...keys: Keys) => Promise<unknown>>(
  store: CacheRegistry,
  fn: Fn | Promise<unknown>,
  keys: Keys = null as unknown as Keys,
  preload = false,
  config: Partial<Config> = {}
) {
  // If no keys were given, the function is the key
  if (keys === null) keys = [fn] as unknown as Keys

  for (const entry of store.entries) {
    // Find a match
    if (shallowEqualArrays(keys, entry.keys, entry.equal)) {
      // If we're pre-loading and the element is present, just return
      if (preload) return undefined as unknown as Await<ReturnType<Fn>>
      // If an error occurred, throw
      if (Object.prototype.hasOwnProperty.call(entry, 'error')) throw entry.error
      // If a response was successful, return
      if (Object.prototype.hasOwnProperty.call(entry, 'response')) {
        if (config.lifespan && config.lifespan > 0) {
          if (entry.timeout) clearTimeout(entry.timeout)
          entry.timeout = setTimeout(entry.remove, config.lifespan)
        }
        return entry.response as Await<ReturnType<Fn>>
      }
      // If the promise is still unresolved, throw
      if (!preload) throw entry.promise
    }
  }

  // The request is new or has changed.
  const entry = new CacheEntry({ keys, equal: config.equal })
  entry.remove = () => {
    const index = store.entries.indexOf(entry)
    if (index !== -1) store.entries.splice(index, 1)
  }
  entry.promise =
    // Execute the promise
    (isPromise(fn) ? fn : fn(...keys))
      // When it resolves, store its value
      .then((response) => {
        entry.response = response
        // Remove the entry in time if a lifespan was given
        if (config.lifespan && config.lifespan > 0) {
          entry.timeout = setTimeout(entry.remove, config.lifespan)
        }
      })
      // Store caught errors, they will be thrown in the render-phase to bubble into an error-bound
      .catch((error) => (entry.error = error))
  // Register the entry
  store.register(entry)
  // And throw the promise, this yields control back to React
  if (!preload) throw entry.promise
  return undefined as unknown as Await<ReturnType<Fn>>
}
