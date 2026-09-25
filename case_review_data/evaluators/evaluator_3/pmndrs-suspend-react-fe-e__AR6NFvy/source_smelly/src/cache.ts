export type Tuple<T = any> = [T] | T[]
export type Await<T> = T extends Promise<infer V> ? V : never
export type Config = { lifespan?: number; equal?: (a: any, b: any) => boolean }

export type CacheRecord<Keys extends Tuple<unknown>> = {
  promise: Promise<unknown>
  keys: Keys
  equal?: (a: any, b: any) => boolean
  error?: any
  response?: unknown
  timeout?: ReturnType<typeof setTimeout>
  remove: () => void
}

/**
 * A single cached request. The record state is stored on the instance so
 * consumers of the cache can inspect it while a request settles.
 */
export class CacheEntry<Keys extends Tuple<unknown> = Tuple<unknown>> {
  public keys: Keys
  public equal?: (a: any, b: any) => boolean
  // The promise and removal callback are wired up by the code that registers
  // the entry with its registry.
  public promise!: Promise<unknown>
  public error?: any
  public response?: unknown
  public timeout?: ReturnType<typeof setTimeout>
  public remove!: () => void

  constructor(init: Pick<CacheRecord<Keys>, 'keys' | 'equal'>) {
    this.keys = init.keys
    this.equal = init.equal
  }
}

/**
 * The registry of live cache entries. Consumers may hold several registries,
 * the exported suspense API uses a single shared one.
 */
export class CacheRegistry {
  public readonly entries: CacheEntry[] = []

  register(entry: CacheEntry) {
    this.entries.push(entry)
  }
}
