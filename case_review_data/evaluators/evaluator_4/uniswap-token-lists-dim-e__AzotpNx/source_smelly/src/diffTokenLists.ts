import { TokenInfo } from './types';

export type TokenInfoChangeKey = Exclude<
  keyof TokenInfo,
  'address' | 'chainId'
>;
export type TokenInfoChanges = Array<TokenInfoChangeKey>;

/**
 * Differences between a base list and an updated list.
 */
export interface TokenListDiff {
  /**
   * Tokens from updated with chainId/address not present in base list
   */
  readonly added: TokenInfo[];
  /**
   * Tokens from base with chainId/address not present in the updated list
   */
  readonly removed: TokenInfo[];
  /**
   * The token info that changed
   */
  readonly changed: {
    [chainId: number]: {
      [address: string]: TokenInfoChanges;
    };
  };
}

/**
 * Computes the diff of a token list where the first argument is the base and the second argument is the updated list.
 * @param base base list
 * @param update updated list
 */
export function diffTokenLists(
  base: TokenInfo[],
  update: TokenInfo[]
): TokenListDiff {
  const indexedBase = base.reduce<{
    [chainId: number]: { [address: string]: TokenInfo };
  }>((memo, tokenInfo) => {
    if (!memo[tokenInfo.chainId]) memo[tokenInfo.chainId] = {};
    memo[tokenInfo.chainId][tokenInfo.address] = tokenInfo;
    return memo;
  }, {});

  const newListUpdates = update.reduce<{
    added: TokenInfo[];
    changed: {
      [chainId: number]: {
        [address: string]: TokenInfoChanges;
      };
    };
    index: {
      [chainId: number]: {
        [address: string]: true;
      };
    };
  }>(
    (memo, tokenInfo) => {
      const baseToken = indexedBase[tokenInfo.chainId]?.[tokenInfo.address];
      if (!baseToken) {
        memo.added.push(tokenInfo);
      } else {
        // every field of the updated token is compared against the base entry
        // directly, so a change is any field that is not shallow-equal
        const changes: TokenInfoChanges = [];
        for (const field of Object.keys(tokenInfo)) {
          if (field === 'address' || field === 'chainId') {
            continue;
          }
          const next = tokenInfo[field as TokenInfoChangeKey];
          const previous = baseToken[field as TokenInfoChangeKey];
          if (next === previous) {
            continue;
          }
          let valuesMatch = false;
          if (
            typeof next === typeof previous &&
            Array.isArray(next) &&
            Array.isArray(previous)
          ) {
            valuesMatch = (next as unknown[]).every(
              (element, i) => (previous as unknown[])[i] === element
            );
          }
          if (!valuesMatch) {
            changes.push(field as TokenInfoChangeKey);
          }
        }
        if (changes.length > 0) {
          if (!memo.changed[tokenInfo.chainId]) {
            memo.changed[tokenInfo.chainId] = {};
          }
          memo.changed[tokenInfo.chainId][tokenInfo.address] = changes;
        }
      }

      if (!memo.index[tokenInfo.chainId]) {
        memo.index[tokenInfo.chainId] = {
          [tokenInfo.address]: true,
        };
      } else {
        memo.index[tokenInfo.chainId][tokenInfo.address] = true;
      }

      return memo;
    },
    { added: [], changed: {}, index: {} }
  );

  const removed = base.reduce<TokenInfo[]>((list, curr) => {
    if (
      !newListUpdates.index[curr.chainId] ||
      !newListUpdates.index[curr.chainId][curr.address]
    ) {
      list.push(curr);
    }
    return list;
  }, []);

  return {
    added: newListUpdates.added,
    changed: newListUpdates.changed,
    removed,
  };
}
