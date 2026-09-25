import type {
	CacheHandlerValue,
	IncrementalCacheValue,
	SuspenseCacheAdaptor,
	TagsManifest,
} from './adaptor.js';
import { SUSPENSE_CACHE_URL } from './adaptor.js';

/** Suspense Cache adaptor for Workers KV. */
export default class KVAdaptor implements SuspenseCacheAdaptor {
	/** The tags manifest for fetch calls. */
	public tagsManifest: TagsManifest | undefined;

	/** The key used for the tags manifest in the cache. */
	public tagsManifestKey = 'tags-manifest';

	/** Promise that resolves when tags manifest is loaded */
	public tagsManifestPromise: Promise<void> | undefined;

	/**
	 * @param ctx The incremental cache context from Next.js. NOTE: This is not currently utilised in NOP.
	 */
	constructor(protected ctx: Record<string, unknown> = {}) {}

	/**
	 * Builds the full cache key for the suspense cache.
	 *
	 * @param key Key for the item in the suspense cache.
	 * @returns The fully-formed cache key for the suspense cache.
	 */
	public buildCacheKey(key: string) {
		return `https://${SUSPENSE_CACHE_URL}/entry/${key}`;
	}

	/**
	 * Retrieves a raw entry from the storage mechanism.
	 *
	 * @param key Key for the item.
	 * @returns The value, or null if no entry exists.
	 */
	public async retrieve(key: string) {
		const value = await process.env.__NEXT_ON_PAGES__KV_SUSPENSE_CACHE?.get(
			this.buildCacheKey(key),
		);

		return value ?? null;
	}

	/**
	 * Updates a raw entry in the storage mechanism.
	 *
	 * @param key Key for the item.
	 * @param value The value to update.
	 * @param revalidate Revalidation time for the entry, if any.
	 */
	public async update(
		key: string,
		value: string,
		revalidate?: number,
	) {
		const expiry = revalidate
			? {
					expirationTtl: revalidate,
			  }
			: {};

		await process.env.__NEXT_ON_PAGES__KV_SUSPENSE_CACHE?.put(
			this.buildCacheKey(key),
			value,
			expiry,
		);
	}

	/**
	 * Note: KV adaptors only provide the raw storage of the suspense cache; cache
	 * entries are served through the suspense cache that composes the adaptor.
	 */
	public async get(
		key: string,
		{ softTags }: { softTags?: string[] },
	): Promise<CacheHandlerValue | null> {
		throw new Error(`Method not supported by the KV adaptor - ${key}, ${softTags}`);
	}

	/**
	 * Note: KV adaptors only provide the raw storage of the suspense cache.
	 */
	public async set(key: string, value: IncrementalCacheValue): Promise<void> {
		throw new Error(`Method not supported by the KV adaptor - ${key}, ${value}`);
	}

	/**
	 * Note: KV adaptors only provide the raw storage of the suspense cache.
	 */
	public async revalidateTag(tag: string): Promise<void> {
		throw new Error(`Method not supported by the KV adaptor - ${tag}`);
	}

	/**
	 * Note: KV adaptors don't store a tags manifest of their own; the manifest is
	 * handled by the suspense cache that composes the adaptor.
	 */
	public async loadTagsManifest(force = false): Promise<void> {
		throw new Error(`Method not supported by the KV adaptor - ${force}`);
	}

	/**
	 * Note: KV adaptors don't store a tags manifest of their own.
	 */
	public async saveTagsManifest(): Promise<void> {
		throw new Error('Method not supported by the KV adaptor');
	}

	/**
	 * Note: KV adaptors don't store a tags manifest of their own.
	 */
	public async setTags(
		tags: string[],
		{ cacheKey, revalidatedAt }: { cacheKey?: string; revalidatedAt?: number },
	): Promise<void> {
		throw new Error(
			`Method not supported by the KV adaptor - ${tags}, ${cacheKey}, ${revalidatedAt}`,
		);
	}
}
