const GIB = 1024 ** 3

export function quotaBytesFromGib(value) {
  const gib = Number(value)
  if (!Number.isFinite(gib) || gib <= 0 || gib > Number.MAX_SAFE_INTEGER / GIB) {
    throw new RangeError('quota must be a positive, safely representable GiB value')
  }
  const bytes = Math.round(gib * GIB)
  if (!Number.isSafeInteger(bytes) || bytes <= 0) {
    throw new RangeError('quota is outside the safely representable byte range')
  }
  return bytes
}
