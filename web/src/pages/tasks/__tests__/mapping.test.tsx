import { useState } from 'react';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { MappingStep } from '../create/MappingStep';
import type { TableMapping } from '../../../lib/validations';

function MappingForm() {
  const [mappings, setMappings] = useState<TableMapping[]>([
    { source_table: 'customers', target_table: 'customers' },
  ]);
  const [batchSize, setBatchSize] = useState('');
  return (
    <MappingStep
      mappings={mappings}
      batchSize={batchSize}
      onChange={setMappings}
      onBatchSizeChange={setBatchSize}
    />
  );
}

describe('MappingStep', () => {
  it('adds and removes mappings while preserving the remaining fields', async () => {
    const user = userEvent.setup();
    render(<MappingForm />);
    expect(screen.getByRole('button', { name: 'tasks.mapping.remove 1' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'tasks.mapping.add' }));
    const secondRow = screen.getAllByRole('group')[1];
    await user.type(within(secondRow).getByLabelText(/tasks.mapping.sourceTable/), 'orders');
    await user.type(within(secondRow).getByLabelText(/tasks.mapping.targetTable/), 'orders');
    await user.click(screen.getByRole('button', { name: 'tasks.mapping.remove 1' }));
    expect(screen.getAllByRole('group')).toHaveLength(1);
    expect(screen.getByLabelText(/tasks.mapping.sourceTable/)).toHaveValue('orders');
    expect(screen.getByRole('button', { name: 'tasks.mapping.remove 1' })).toBeDisabled();
  });

  it('explains duplicate target tables and invalid batch sizes', async () => {
    const user = userEvent.setup();
    render(<MappingForm />);
    await user.click(screen.getByRole('button', { name: 'tasks.mapping.add' }));
    const secondRow = screen.getAllByRole('group')[1];
    await user.type(within(secondRow).getByLabelText(/tasks.mapping.sourceTable/), 'orders');
    await user.type(within(secondRow).getByLabelText(/tasks.mapping.targetTable/), 'customers');
    expect(screen.getByText('tasks.mapping.duplicateTarget')).toBeInTheDocument();
    await user.type(screen.getByLabelText('tasks.mapping.batchSize'), '0');
    expect(screen.getByText('tasks.mapping.batchInvalid')).toBeInTheDocument();
  });
});
