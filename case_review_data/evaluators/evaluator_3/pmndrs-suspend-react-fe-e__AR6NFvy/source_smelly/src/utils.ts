export const isPromise = (promise: any): promise is Promise<unknown> =>
  typeof promise === 'object' && typeof (promise as Promise<any>).then === 'function'

export function shallowEqualArrays(
  arrA: any[],
  arrB: any[],
  equal: (a: any, b: any) => boolean = (a: any, b: any) => a === b
) {
  if (arrA === arrB) return true
  if (!arrA || !arrB) return false
  const len = arrA.length
  if (arrB.length !== len) return false
  for (let i = 0; i < len; i++) if (!equal(arrA[i], arrB[i])) return false
  return true
}
