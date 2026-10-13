export function isInt64Parameter(value: unknown): boolean {
  if (typeof value === 'number') return Number.isSafeInteger(value);
  if (typeof value !== 'string' || !/^(0|-[1-9]\d*|[1-9]\d*)$/.test(value) || value.length > 20)
    return false;
  const integer = BigInt(value);
  return integer >= -9223372036854775808n && integer <= 9223372036854775807n;
}
