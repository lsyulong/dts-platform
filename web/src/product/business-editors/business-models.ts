export const emptyValue = (value: unknown) => value === undefined || value === null || value === '';

export function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function stringList(value: unknown): string[] | null {
  if (emptyValue(value)) return [];
  return Array.isArray(value) && value.every((item) => typeof item === 'string') ? value : null;
}

export function duplicateIndexes(values: string[]): Set<number> {
  const names = values.map((value) => value.trim());
  return new Set(
    names.flatMap((name, index) =>
      name && names.indexOf(name) !== names.lastIndexOf(name) ? [index] : [],
    ),
  );
}

export function sensitiveKey(key: string) {
  const normalized = key
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1_$2')
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2');
  return /(^|[_\-.])(password|passwd|pwd|secret|token|api[_-]?key|private[_-]?key|salt|credential)([_\-.]|$)/i.test(
    normalized,
  );
}

export const credentialReferenceKey = (key: string) => /_credential_ref$/i.test(key);
export const credentialReference = (value: unknown) =>
  typeof value === 'string' &&
  value.startsWith('credential:') &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
    value.slice('credential:'.length),
  );
