import { useState } from 'react';
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import ActionParameters from '../ActionParameters';
import SqlParametersEditor from '../business-editors/SqlParametersEditor';
import { isInt64Parameter } from '../int64-parameter';
import type { ProductResource } from '../resource-types';

const record: ProductResource = {
  id: 'api',
  tenant_id: 'tenant',
  kind: 'data_api',
  code: 'api',
  name: 'API',
  description: null,
  parent_id: null,
  status: 'published',
  version: 1,
  created_by: 'user',
  created_at: '2026-10-09T00:00:00Z',
  updated_at: '2026-10-09T00:00:00Z',
  references: [],
  properties: { parameters: [{ name: 'id', data_type: 'BIGINT', required: true }] },
};
function Input() {
  const [value, setValue] = useState<Record<string, unknown>>({});
  return (
    <>
      <ActionParameters
        record={record}
        value={value}
        onChange={setValue}
        disabled={false}
        text={(zh) => zh}
      />
      <output>{JSON.stringify(value)}</output>
    </>
  );
}
function Defaults() {
  const [value, setValue] = useState<unknown>([
    { name: 'id', data_type: 'BIGINT', required: true, default_value: '9223372036854775807' },
  ]);
  return (
    <>
      <SqlParametersEditor
        kind="data_api"
        fieldKey="parameters"
        value={value}
        onChange={setValue}
      />
      <output>{JSON.stringify(value)}</output>
    </>
  );
}
describe('exact BIGINT browser transport', () => {
  it('preserves integers above the JavaScript safe range in actual request input', async () => {
    const user = userEvent.setup();
    render(<Input />);
    await user.type(screen.getByLabelText(/id · BIGINT/), '9007199254740993');
    expect(screen.getByRole('status')).toHaveTextContent('"id":"9007199254740993"');
  });
  it('keeps exact saved defaults editable without numeric conversion', async () => {
    const user = userEvent.setup();
    render(<Defaults />);
    const input = screen.getByDisplayValue('9223372036854775807');
    expect(input).toBeValid();
    await user.clear(input);
    await user.type(input, '-9223372036854775808');
    expect(input).toBeValid();
    expect(screen.getByRole('status')).toHaveTextContent('"default_value":"-9223372036854775808"');
    await user.type(input, '0');
    expect(input).toBeInvalid();
  });
  it('rejects out-of-range and ambiguous textual or rounded numeric values', () => {
    for (const value of [
      '9223372036854775808',
      '-9223372036854775809',
      '-0',
      '01',
      '+1',
      '1;DROP TABLE x',
      9007199254740992,
    ])
      expect(isInt64Parameter(value)).toBe(false);
    for (const value of [
      '0',
      '9007199254740993',
      '9223372036854775807',
      '-9223372036854775808',
      42,
    ])
      expect(isInt64Parameter(value)).toBe(true);
  });
});
