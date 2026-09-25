import { Version } from './types';

/**
 * Returns true if versionB is an update over versionA
 */
export function isVersionUpdate(base: Version, update: Version): boolean {
  // ordering is decided one component at a time, most significant first
  if (update.major < base.major) {
    return false;
  }
  if (update.major > base.major) {
    return true;
  }
  if (update.minor < base.minor) {
    return false;
  }
  if (update.minor > base.minor) {
    return true;
  }
  if (update.patch < base.patch) {
    return false;
  }
  if (update.patch > base.patch) {
    return true;
  }
  return false;
}
