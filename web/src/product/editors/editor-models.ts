export interface StructuredEditorProps {
  value: unknown;
  onChange: (value: unknown) => void;
  disabled?: boolean;
}

export interface ModelColumn {
  name: string;
  data_type: string;
  nullable: boolean;
  description: string;
  standard_id?: unknown;
  standard_version?: unknown;
  naming_standard_id?: unknown;
  naming_standard_version?: unknown;
}

export interface EnumValue {
  value: string;
  label: string;
  description: string;
}

export const ruleOperators = [
  'eq',
  'ne',
  'gt',
  'gte',
  'lt',
  'lte',
  'is_null',
  'not_null',
  'in',
] as const;
export type RuleOperator = (typeof ruleOperators)[number];

export interface RuleCondition {
  column: string;
  operator: RuleOperator;
  value: unknown;
}

export interface WorkflowNode {
  id: string;
  name: string;
  sql: string;
  depends_on: string[];
}

export interface WorkflowDefinition {
  nodes: WorkflowNode[];
  parameters?: unknown;
}

export function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

const text = (value: unknown) => (typeof value === 'string' ? value : '');
const items = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);

export function modelColumns(value: unknown): ModelColumn[] {
  return items(value).map((item) => {
    const row = asRecord(item);
    return {
      name: text(row.name),
      data_type: text(row.data_type),
      nullable: row.nullable !== false,
      description: text(row.description),
      ...Object.fromEntries(
        ['standard_id', 'standard_version', 'naming_standard_id', 'naming_standard_version']
          .filter((key) => Object.hasOwn(row, key))
          .map((key) => [key, row[key]]),
      ),
    };
  });
}

export function enumValues(value: unknown): EnumValue[] {
  return items(value).map((item) => {
    const row = asRecord(item);
    return {
      value: text(row.value),
      label: text(row.label),
      description: text(row.description),
    };
  });
}

export function ruleConditions(value: unknown): RuleCondition[] {
  return items(value).map((item) => {
    const row = asRecord(item);
    const operator = ruleOperators.find((operator) => operator === row.operator) ?? 'eq';
    return { column: text(row.column), operator, value: row.value === undefined ? '' : row.value };
  });
}

export function workflowDefinition(value: unknown): WorkflowDefinition {
  const graph = asRecord(value);
  return {
    ...(graph.parameters === undefined ? {} : { parameters: graph.parameters }),
    nodes: items(graph.nodes).map((item) => {
      const node = asRecord(item);
      return {
        id: text(node.id),
        name: text(node.name),
        sql: text(node.sql),
        depends_on: items(node.depends_on).filter((id): id is string => typeof id === 'string'),
      };
    }),
  };
}

export function duplicateValues(values: string[], caseInsensitive = false): string[] {
  const seen = new Set<string>();
  const duplicates = new Set<string>();
  for (const value of values) {
    const key = caseInsensitive ? value.trim().toLowerCase() : value.trim();
    if (!key) continue;
    if (seen.has(key)) duplicates.add(key);
    seen.add(key);
  }
  return [...duplicates];
}

export function findWorkflowCycle(nodes: WorkflowNode[]): string[] {
  const edges = new Map(nodes.map((node) => [node.id, node.depends_on]));
  const states = new Map<string, 'visiting' | 'visited'>();
  for (const id of edges.keys()) {
    if (states.has(id)) continue;
    const stack = [{ id, index: 0 }];
    states.set(id, 'visiting');
    while (stack.length) {
      const frame = stack[stack.length - 1];
      const dependencies = edges.get(frame.id) ?? [];
      if (frame.index === dependencies.length) {
        states.set(frame.id, 'visited');
        stack.pop();
        continue;
      }
      const dependency = dependencies[frame.index];
      frame.index += 1;
      if (!edges.has(dependency) || states.get(dependency) === 'visited') continue;
      if (states.get(dependency) === 'visiting') {
        const start = stack.findIndex((item) => item.id === dependency);
        return [...stack.slice(start).map((item) => item.id), dependency];
      }
      states.set(dependency, 'visiting');
      stack.push({ id: dependency, index: 0 });
    }
  }
  return [];
}

export function missingWorkflowDependencies(nodes: WorkflowNode[]): string[] {
  const ids = new Set(nodes.map((node) => node.id));
  return [...new Set(nodes.flatMap((node) => node.depends_on.filter((id) => !ids.has(id))))];
}

export function removeWorkflowNode(
  definition: WorkflowDefinition,
  id: string,
  index?: number,
): WorkflowDefinition {
  const remaining = definition.nodes.filter((node, position) =>
    index === undefined ? node.id !== id : position !== index,
  );
  const removeReferences = !remaining.some((node) => node.id === id);
  return {
    ...definition,
    nodes: remaining.map((node) => ({
      ...node,
      depends_on: removeReferences
        ? node.depends_on.filter((dependency) => dependency !== id)
        : node.depends_on,
    })),
  };
}

export function nextWorkflowNodeId(nodes: WorkflowNode[]): string {
  const ids = new Set(nodes.map((node) => node.id));
  let index = 1;
  while (ids.has(`node_${index}`)) index += 1;
  return `node_${index}`;
}
