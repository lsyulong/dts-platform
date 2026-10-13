import type { ProductResource } from '../resource-types';

export interface ResourceTree {
  roots: ProductResource[];
  children: Map<string, ProductResource[]>;
  missingParents: Set<string>;
  cycles: Set<string>;
}

export function buildResourceTree(records: ProductResource[]): ResourceTree {
  const byId = new Map(records.map((record) => [record.id, record]));
  const cycles = new Set<string>();
  const missingParents = new Set<string>();
  const children = new Map<string, ProductResource[]>();

  for (const record of records) {
    const path: string[] = [];
    const positions = new Map<string, number>();
    let current: ProductResource | undefined = record;
    while (current) {
      const repeatedAt = positions.get(current.id);
      if (repeatedAt !== undefined) {
        path.slice(repeatedAt).forEach((id) => cycles.add(id));
        break;
      }
      positions.set(current.id, path.length);
      path.push(current.id);
      current = current.parent_id ? byId.get(current.parent_id) : undefined;
    }
  }

  const roots: ProductResource[] = [];
  for (const record of records) {
    if (record.parent_id && !byId.has(record.parent_id)) missingParents.add(record.id);
    if (!record.parent_id || missingParents.has(record.id) || cycles.has(record.id)) {
      roots.push(record);
    } else {
      const siblings = children.get(record.parent_id) ?? [];
      siblings.push(record);
      children.set(record.parent_id, siblings);
    }
  }
  return { roots, children, missingParents, cycles };
}
