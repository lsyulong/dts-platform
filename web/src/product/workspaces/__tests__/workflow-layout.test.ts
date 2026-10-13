import { describe, expect, it } from 'vitest';
import { workflowLayers } from '../workflow-layout';
import type { WorkflowNode } from '../../editors/editor-models';

const node = (id: string, depends_on: string[] = []): WorkflowNode => ({
  id,
  name: id,
  sql: '',
  depends_on,
});

describe('Workflow dependency layout', () => {
  it('places dependencies before dependants regardless of saved node order', () => {
    const nodes = [
      node('sink', ['a', 'b']),
      node('a', ['source']),
      node('source'),
      node('b', ['source']),
    ];
    expect(workflowLayers(nodes).map((layer) => layer.map((item) => item.id))).toEqual([
      ['source'],
      ['a', 'b'],
      ['sink'],
    ]);
    expect(nodes.map((item) => item.id)).toEqual(['sink', 'a', 'source', 'b']);
  });

  it('places nodes without dependencies in the same initial layer', () => {
    expect(
      workflowLayers([node('a'), node('b')]).map((layer) => layer.map((item) => item.id)),
    ).toEqual([['a', 'b']]);
  });

  it('rejects cyclic, missing and duplicate node configurations instead of fabricating a DAG', () => {
    expect(workflowLayers([node('a', ['b']), node('b', ['a'])])).toEqual([]);
    expect(workflowLayers([node('a', ['missing'])])).toEqual([]);
    expect(workflowLayers([node('a'), node('a')])).toEqual([]);
    expect(workflowLayers([node('')])).toEqual([]);
  });

  it('counts duplicate dependency references once when computing node order', () => {
    expect(
      workflowLayers([node('a'), node('b', ['a', 'a'])]).map((layer) =>
        layer.map((item) => item.id),
      ),
    ).toEqual([['a'], ['b']]);
  });
});
