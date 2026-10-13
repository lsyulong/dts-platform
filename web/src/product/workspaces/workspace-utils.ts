import type { ResourceSchema } from '../resource-types';
import { formatProperty } from '../format-property';

export function firstField(schema: ResourceSchema, keys: string[]) {
  for (const key of keys) {
    const field = schema.fields.find((item) => item.key === key);
    if (field) return field;
  }
  return undefined;
}

export function readableValue(value: unknown, language: string): string {
  if (Array.isArray(value))
    return value.map((item) => formatProperty(item, language)).join(', ') || '—';
  return formatProperty(value, language);
}
