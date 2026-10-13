import { useState } from 'react';
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import ModelColumnsEditor from '../ModelColumnsEditor';
import WorkflowGraphEditor from '../WorkflowGraphEditor';

function ColumnsHarness() {
  const [value, setValue] = useState<unknown>([
    { name: 'order_id', data_type: 'BIGINT', nullable: false, description: '' },
    { name: 'amount', data_type: 'DECIMAL(18,2)', nullable: true, description: '' },
  ]);
  return <ModelColumnsEditor value={value} onChange={setValue} />;
}

function WorkflowHarness() {
  const [value, setValue] = useState<unknown>({
    nodes: [
      { id: 'a', name: 'Source', sql: 'SELECT 1', depends_on: ['b'] },
      { id: 'b', name: 'Sink', sql: 'SELECT 2', depends_on: ['a'] },
    ],
  });
  return (
    <>
      <WorkflowGraphEditor value={value} onChange={setValue} />
      <output data-testid="definition">{JSON.stringify(value)}</output>
    </>
  );
}

function BoundColumnsHarness() {
  const [value, setValue] = useState<unknown>([
    { name: 'amount', data_type: 'DECIMAL(18,2)', nullable: true, description: '',
      standard_id: 'field-standard', standard_version: 3,
      naming_standard_id: 'column-naming', naming_standard_version: 2 },
  ]);
  return <><ModelColumnsEditor value={value} onChange={setValue} />
    <output data-testid="bound-columns">{JSON.stringify(value)}</output></>;
}

describe('Structured business configuration interactions', () => {
  it('retains exact standard revision pins when an existing column is edited', async () => {
    const user = userEvent.setup();
    render(<BoundColumnsHarness />);
    await user.click(screen.getByLabelText('必填 1'));
    await user.type(screen.getByLabelText('业务说明 1'), 'Paid amount');
    expect(JSON.parse(screen.getByTestId('bound-columns').textContent ?? '')).toEqual([
      { name: 'amount', data_type: 'DECIMAL(18,2)', nullable: false, description: 'Paid amount',
        standard_id: 'field-standard', standard_version: 3,
        naming_standard_id: 'column-naming', naming_standard_version: 2 },
    ]);
  });
  it('shows duplicate column names and clears the warning after removing the duplicate', async () => {
    const user = userEvent.setup();
    render(<ColumnsHarness />);
    await user.clear(screen.getByLabelText('字段名称 2'));
    await user.type(screen.getByLabelText('字段名称 2'), 'ORDER_ID');
    expect(screen.getByRole('alert')).toHaveTextContent('字段名称重复');
    expect(screen.getByLabelText('字段名称 2')).toHaveAttribute('aria-invalid', 'true');
    await user.click(screen.getByRole('button', { name: '删除字段 ORDER_ID' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.getByLabelText('必填 1')).toBeChecked();
  });

  it('reports a workflow cycle and removes dependency references when a node is deleted', async () => {
    const user = userEvent.setup();
    render(<WorkflowHarness />);
    expect(screen.getByRole('alert')).toHaveTextContent('存在环依赖');
    await user.click(screen.getByRole('button', { name: '删除节点 Source' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(JSON.parse(screen.getByTestId('definition').textContent ?? '')).toEqual({
      nodes: [{ id: 'b', name: 'Sink', sql: 'SELECT 2', depends_on: [] }],
    });
  });
});
