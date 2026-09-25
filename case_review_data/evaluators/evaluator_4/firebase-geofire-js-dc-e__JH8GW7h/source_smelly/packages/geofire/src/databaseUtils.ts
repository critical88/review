import { Document } from './GeoFireTypes';
import { validateLocation, validateGeohash, Geopoint, Geohash } from "geofire-common";
import { DataSnapshot } from 'firebase/database';

/**
 * Encodes a location and geohash as a GeoFire object.
 *
 * @param latitude The latitude part of the location.
 * @param longitude The longitude part of the location.
 * @param geohash The geohash of the location.
 * @returns The location encoded as GeoFire object.
 */
export function encodeGeoFireObject(latitude: number, longitude: number, geohash: Geohash): Document {
  validateLocation([latitude, longitude]);
  validateGeohash(geohash);
  return { '.priority': geohash, 'g': geohash, 'l': [latitude, longitude] };
}

/**
 * Decodes the location given as GeoFire object. Returns null if decoding fails.
 *
 * @param geoFireObj The location encoded as GeoFire object.
 * @returns The location as [latitude, longitude] pair or null if decoding fails.
 */
export function decodeGeoFireObject(geoFireObj: Document): Geopoint {
  if (geoFireObj && 'l' in geoFireObj && Array.isArray(geoFireObj.l) && geoFireObj.l.length === 2) {
    return geoFireObj.l as Geopoint;
  } else {
    throw new Error('Unexpected location object encountered: ' + JSON.stringify(geoFireObj));
  }
}

/**
 * Returns the key of a Firebase snapshot across SDK versions.
 *
 * @param A Firebase snapshot.
 * @returns The Firebase snapshot's key.
 */
export function geoFireGetKey(snapshot: DataSnapshot): string {
  let key;
  if (typeof snapshot.key === 'string' || snapshot.key === null) {
    key = snapshot.key;
  } else if (typeof snapshot.key === 'function') {
    // @ts-ignore
    key = snapshot.key();
  } else {
    // @ts-ignore
    key = snapshot.name();
  }

  return key;
}
