import type { WorkflowNode } from '../editors/editor-models';

export function workflowLayers(nodes: WorkflowNode[]): WorkflowNode[][] {
  const ids = new Set(nodes.map((node) => node.id));
  if (
    ids.size !== nodes.length ||
    nodes.some((node) => !node.id.trim() || node.depends_on.some((id) => !ids.has(id)))
  )
    return [];
  const remaining = new Map(
    nodes.map((node) => [node.id, new Set(node.depends_on.filter((id) => ids.has(id))).size]),
  );
  const dependants = new Map<string, string[]>();
  const depths = new Map(nodes.map((node) => [node.id, 0]));
  for (const node of nodes) {
    for (const id of new Set(node.depends_on)) {
      if (!ids.has(id)) continue;
      dependants.set(id, [...(dependants.get(id) ?? []), node.id]);
    }
  }
  const queue = nodes.filter((node) => remaining.get(node.id) === 0).map((node) => node.id);
  for (let position = 0; position < queue.length; position += 1) {
    const id = queue[position];
    for (const child of dependants.get(id) ?? []) {
      depths.set(child, Math.max(depths.get(child) ?? 0, (depths.get(id) ?? 0) + 1));
      const count = (remaining.get(child) ?? 1) - 1;
      remaining.set(child, count);
      if (count === 0) queue.push(child);
    }
  }
  if (queue.length !== nodes.length) return [];
  const layers: WorkflowNode[][] = [];
  for (const node of nodes) {
    const depth = depths.get(node.id) ?? 0;
    layers[depth] = [...(layers[depth] ?? []), node];
  }
  return layers;
}
