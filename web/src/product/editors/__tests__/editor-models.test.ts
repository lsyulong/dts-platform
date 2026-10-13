import { describe, expect, it } from 'vitest';
import {
  duplicateValues,
  findWorkflowCycle,
  missingWorkflowDependencies,
  nextWorkflowNodeId,
  removeWorkflowNode,
  type WorkflowDefinition,
  type WorkflowNode,
} from '../editor-models';
import { hasStructuredPropertyEditor } from '../StructuredPropertyEditor';

const node = (id: string, depends_on: string[] = []): WorkflowNode => ({
  id,
  name: id,
  sql: `SELECT '${id}'`,
  depends_on,
});

describe('Structured editor business rules', () => {
  it('detects duplicate model columns after trimming and ignoring SQL identifier case', () => {
    expect(duplicateValues(['Order_ID', ' order_id ', 'amount', '', '   '], true)).toEqual([
      'order_id',
    ]);
    expect(duplicateValues(['Paid', 'paid'])).toEqual([]);
  });

  it('accepts an acyclic dependency diamond and identifies missing references separately', () => {
    const nodes = [
      node('source'),
      node('a', ['source']),
      node('b', ['source']),
      node('sink', ['a', 'b', 'deleted']),
    ];
    expect(findWorkflowCycle(nodes)).toEqual([]);
    expect(missingWorkflowDependencies(nodes)).toEqual(['deleted']);
  });

  it('reports the dependency cycle including its closing edge', () => {
    expect(findWorkflowCycle([node('a', ['b']), node('b', ['c']), node('c', ['a'])])).toEqual([
      'a',
      'b',
      'c',
      'a',
    ]);
    expect(findWorkflowCycle([node('self', ['self'])])).toEqual(['self', 'self']);
  });

  it('removes a deleted node and all references while preserving the other SQL definitions', () => {
    const definition: WorkflowDefinition = {
      nodes: [node('source'), node('a', ['source']), node('sink', ['source', 'a'])],
    };
    const result = removeWorkflowNode(definition, 'source');
    expect(result.nodes).toEqual([node('a'), node('sink', ['a'])]);
    expect(definition.nodes[2].depends_on).toEqual(['source', 'a']);
  });

  it('deletes only the selected duplicate node and keeps still-valid references', () => {
    const definition = { nodes: [node('a'), node('a'), node('b', ['a'])] };
    expect(removeWorkflowNode(definition, 'a', 0).nodes).toEqual([node('a'), node('b', ['a'])]);
  });

  it('creates a new node identifier without colliding with imported identifiers', () => {
    expect(nextWorkflowNodeId([node('node_1'), node('node_2'), node('node_4')])).toBe('node_3');
  });

  it('dispatches only the agreed structured fields and leaves other JSON semantics unchanged', () => {
    expect(hasStructuredPropertyEditor('data_model', 'columns')).toBe(true);
    expect(hasStructuredPropertyEditor('system_dictionary', 'enum_values')).toBe(true);
    expect(hasStructuredPropertyEditor('quality_rule', 'conditions')).toBe(true);
    expect(hasStructuredPropertyEditor('development_workflow', 'definition')).toBe(true);
    expect(hasStructuredPropertyEditor('data_access_request', 'columns')).toBe(false);
    expect(hasStructuredPropertyEditor('system_setting', 'values')).toBe(false);
  });
});
