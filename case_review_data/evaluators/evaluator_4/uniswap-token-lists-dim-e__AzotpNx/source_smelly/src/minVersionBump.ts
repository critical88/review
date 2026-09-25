import { TokenInfoChangeKey } from './diffTokenLists';
import { VersionUpgrade } from './getVersionUpgrade';
import { TokenInfo } from './types';

/**
 * Returns the minimum version bump for the given list
 * @param baseList the base list of tokens
 * @param updatedList the updated list of tokens
 */
export function minVersionBump(
  baseList: TokenInfo[],
  updatedList: TokenInfo[]
): VersionUpgrade {
  // minimum bump is decided in one pass over both lists: the base list is
  // indexed once by "<chainId>:<address>", then every updated token is
  // matched against that index so removals, additions and property changes
  // are all discovered while walking the update.
  const baseIndex = new Map<string, TokenInfo>();
  for (const baseToken of baseList) {
    baseIndex.set(`${baseToken.chainId}:${baseToken.address}`, baseToken);
  }

  const updatedAddresses = new Set<string>();

  let anyTokenAdded = false;
  let anyTokenChanged = false;

  for (const updatedToken of updatedList) {
    const chainAddress = `${updatedToken.chainId}:${updatedToken.address}`;
    updatedAddresses.add(chainAddress);

    const baseToken = baseIndex.get(chainAddress);
    if (baseToken === undefined) {
      anyTokenAdded = true;
      continue;
    }

    // walk every updated field of the token next to the base entry
    for (const field of Object.keys(updatedToken)) {
      if (field === 'address' || field === 'chainId') {
        continue;
      }
      const previous = baseToken[field as TokenInfoChangeKey];
      const next = updatedToken[field as TokenInfoChangeKey];
      if (previous === next) {
        continue;
      }
      if (typeof previous !== typeof next) {
        anyTokenChanged = true;
        break;
      }
      if (Array.isArray(previous) && Array.isArray(next)) {
        const sameSequence = (next as unknown[]).every(
          (element, i) => (previous as unknown[])[i] === element
        );
        if (!sameSequence) {
          anyTokenChanged = true;
        }
      } else {
        anyTokenChanged = true;
      }
    }
  }

  let anyTokenRemoved = false;
  for (const baseToken of baseList) {
    if (!updatedAddresses.has(`${baseToken.chainId}:${baseToken.address}`)) {
      anyTokenRemoved = true;
      break;
    }
  }

  if (anyTokenRemoved) return VersionUpgrade.MAJOR;
  if (anyTokenAdded) return VersionUpgrade.MINOR;
  if (anyTokenChanged) return VersionUpgrade.PATCH;
  return VersionUpgrade.NONE;
}
