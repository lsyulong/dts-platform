import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import BusinessPropertyEditor, { hasBusinessPropertyEditor } from '../../BusinessPropertyEditor';

function EditorForm({
  kind,
  fieldKey,
  initialValue,
  onChange,
  onSubmit,
}: {
  kind: string;
  fieldKey: string;
  initialValue: unknown;
  onChange: (next: unknown) => void;
  onSubmit?: () => void;
}) {
  const [value, setValue] = useState(initialValue);
  return (
    <form
      aria-label="业务配置"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit?.();
      }}
    >
      <BusinessPropertyEditor
        kind={kind}
        fieldKey={fieldKey}
        value={value}
        onChange={(next) => {
          setValue(next);
          onChange(next);
        }}
      />
      <button type="submit">保存配置</button>
    </form>
  );
}

describe('Business property editing', () => {
  it('adds and removes list items and blocks native form submission while values repeat', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onSubmit = vi.fn();
    render(
      <EditorForm
        kind="realtime_task"
        fieldKey="primary_key"
        initialValue={['customer_id']}
        onChange={onChange}
        onSubmit={onSubmit}
      />,
    );

    await user.click(screen.getByRole('button', { name: '添加一项' }));
    expect(onChange).toHaveBeenLastCalledWith(['customer_id', '']);
    await user.type(screen.getByLabelText('项目 2'), ' customer_id ');
    expect(onChange).toHaveBeenLastCalledWith(['customer_id', ' customer_id ']);
    expect(screen.getByRole('alert')).toHaveTextContent('内容重复');
    expect(screen.getByRole('form', { name: '业务配置' })).toBeInvalid();
    await user.click(screen.getByRole('button', { name: '保存配置' }));
    expect(onSubmit).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: '移除 项目 2' }));
    expect(onChange).toHaveBeenLastCalledWith(['customer_id']);
    expect(screen.queryByLabelText('项目 2')).not.toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.getByRole('form', { name: '业务配置' })).toBeValid();
    await user.click(screen.getByRole('button', { name: '保存配置' }));
    expect(onSubmit).toHaveBeenCalledOnce();
  });

  it('emits mappings as an ordered array with separate source and target column values', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <EditorForm
        kind="realtime_task"
        fieldKey="mapping"
        initialValue={[{ source_column: 'source_id', target_column: 'id' }]}
        onChange={onChange}
      />,
    );

    await user.clear(screen.getByLabelText('源字段 1'));
    await user.type(screen.getByLabelText('源字段 1'), 'source_order_id');
    await user.clear(screen.getByLabelText('目标字段 1'));
    await user.type(screen.getByLabelText('目标字段 1'), 'order_id');
    await user.click(screen.getByRole('button', { name: '添加一项' }));
    await user.type(screen.getByLabelText('源字段 2'), 'amount');
    await user.type(screen.getByLabelText('目标字段 2'), 'total_amount');

    expect(onChange).toHaveBeenLastCalledWith([
      { source_column: 'source_order_id', target_column: 'order_id' },
      { source_column: 'amount', target_column: 'total_amount' },
    ]);
    expect(screen.getByRole('form', { name: '业务配置' })).toBeValid();
  });

  it.each(['sql_document', 'data_api'])(
    'preserves false and zero defaults separately from absent and NULL defaults for %s',
    async (kind) => {
      const user = userEvent.setup();
      const onChange = vi.fn();
      render(
        <EditorForm
          kind={kind}
          fieldKey="parameters"
          initialValue={[
            { name: 'enabled', data_type: 'BOOLEAN', required: false, default_value: false },
            { name: 'limit', data_type: 'BIGINT', required: false, default_value: 0 },
            { name: 'filter', data_type: 'VARCHAR', required: false },
            { name: 'cursor', data_type: 'VARCHAR', required: false, default_value: null },
          ]}
          onChange={onChange}
        />,
      );

      expect(screen.getByLabelText('默认值方式 1')).toHaveValue('value');
      expect(screen.getByLabelText('默认值 1')).toHaveValue('false');
      expect(screen.getByLabelText('默认值方式 2')).toHaveValue('value');
      expect(screen.getByLabelText('默认值 2')).toHaveValue('0');
      expect(screen.getByLabelText('默认值方式 3')).toHaveValue('none');
      expect(screen.getByLabelText('默认值方式 4')).toHaveValue('null');

      await user.click(screen.getByLabelText('必填 1'));
      expect(onChange).toHaveBeenLastCalledWith([
        { name: 'enabled', data_type: 'BOOLEAN', required: true, default_value: false },
        { name: 'limit', data_type: 'BIGINT', required: false, default_value: 0 },
        { name: 'filter', data_type: 'VARCHAR', required: false },
        { name: 'cursor', data_type: 'VARCHAR', required: false, default_value: null },
      ]);

      await user.selectOptions(screen.getByLabelText('默认值方式 1'), 'none');
      await user.selectOptions(screen.getByLabelText('默认值方式 2'), 'null');
      expect(onChange).toHaveBeenLastCalledWith([
        { name: 'enabled', data_type: 'BOOLEAN', required: true },
        { name: 'limit', data_type: 'BIGINT', required: false, default_value: null },
        { name: 'filter', data_type: 'VARCHAR', required: false },
        { name: 'cursor', data_type: 'VARCHAR', required: false, default_value: null },
      ]);
      expect(screen.queryByLabelText('默认值 1')).not.toBeInTheDocument();
      expect(screen.queryByLabelText('默认值 2')).not.toBeInTheDocument();
      expect(screen.getByRole('form', { name: '业务配置' })).toBeValid();
    },
  );

  it('hides camelCase and underscore secrets and emits only explicitly migrated credential references', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onSubmit = vi.fn();
    const { container } = render(
      <EditorForm
        kind="system_message"
        fieldKey="parameters"
        initialValue={{
          dbPassword: 'synthetic-db-secret',
          api_key: 'synthetic-api-secret',
          retry_count: 1,
        }}
        onChange={onChange}
        onSubmit={onSubmit}
      />,
    );

    expect(screen.getByLabelText('名称 1')).toHaveValue('dbPassword');
    expect(screen.getByLabelText('名称 2')).toHaveValue('api_key');
    expect(container.innerHTML).not.toContain('synthetic-db-secret');
    expect(container.innerHTML).not.toContain('synthetic-api-secret');
    expect(screen.queryByDisplayValue('synthetic-db-secret')).not.toBeInTheDocument();
    expect(screen.queryByDisplayValue('synthetic-api-secret')).not.toBeInTheDocument();
    expect(screen.getByRole('form', { name: '业务配置' })).toBeInvalid();

    await user.clear(screen.getByLabelText('值 3'));
    await user.type(screen.getByLabelText('值 3'), '2');
    await user.click(screen.getByRole('button', { name: '保存配置' }));
    expect(onChange).not.toHaveBeenCalled();
    expect(onSubmit).not.toHaveBeenCalled();

    await user.click(screen.getAllByRole('button', { name: '迁移为凭据引用' })[0]);
    expect(screen.getByLabelText('名称 1')).toHaveValue('dbPassword_credential_ref');
    expect(screen.getByLabelText('凭据引用 ID 1')).toHaveValue('');
    await user.type(
      screen.getByLabelText('凭据引用 ID 1'),
      'Credential:AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA',
    );
    expect(screen.getByLabelText('凭据引用 ID 1')).toBeInvalid();
    expect(screen.getByRole('form', { name: '业务配置' })).toBeInvalid();
    expect(onChange).not.toHaveBeenCalled();
    await user.clear(screen.getByLabelText('凭据引用 ID 1'));
    await user.type(
      screen.getByLabelText('凭据引用 ID 1'),
      'credential:AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA',
    );
    expect(screen.getByLabelText('凭据引用 ID 1')).toBeValid();
    expect(onChange).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: '迁移为凭据引用' }));
    expect(screen.getByLabelText('名称 2')).toHaveValue('api_key_credential_ref');
    expect(screen.getByLabelText('凭据引用 ID 2')).toHaveValue('');
    await user.type(
      screen.getByLabelText('凭据引用 ID 2'),
      'credential:bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
    );
    expect(onChange).toHaveBeenCalledOnce();
    expect(onChange).toHaveBeenLastCalledWith({
      dbPassword_credential_ref: 'credential:AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA',
      api_key_credential_ref: 'credential:bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
      retry_count: 2,
    });
    expect(screen.getByRole('form', { name: '业务配置' })).toBeValid();
    await user.click(screen.getByRole('button', { name: '保存配置' }));
    expect(onSubmit).toHaveBeenCalledOnce();
  });

  it('emits ordinary numeric and boolean values with their scalar types', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <EditorForm
        kind="system_message"
        fieldKey="parameters"
        initialValue={{ attempts: 3, enabled: true, label: 'demo' }}
        onChange={onChange}
      />,
    );

    await user.clear(screen.getByLabelText('值 1'));
    await user.type(screen.getByLabelText('值 1'), '0');
    await user.selectOptions(screen.getByLabelText('值 2'), 'false');
    expect(onChange).toHaveBeenLastCalledWith({ attempts: 0, enabled: false, label: 'demo' });

    await user.selectOptions(screen.getByLabelText('值类型 3'), 'number');
    expect(screen.getByRole('form', { name: '业务配置' })).toBeInvalid();
    await user.type(screen.getByLabelText('值 3'), '2.5');
    expect(onChange).toHaveBeenLastCalledWith({ attempts: 0, enabled: false, label: 2.5 });
    expect(screen.getByRole('form', { name: '业务配置' })).toBeValid();
  });

  it('leaves checkpoints and model columns to their existing editors', () => {
    expect(hasBusinessPropertyEditor('realtime_task', 'checkpoint')).toBe(false);
    expect(hasBusinessPropertyEditor('data_model', 'columns')).toBe(false);
    expect(hasBusinessPropertyEditor('master_data_model', 'columns')).toBe(false);
    expect(hasBusinessPropertyEditor('data_access_request', 'columns')).toBe(true);
    expect(hasBusinessPropertyEditor('quality_object', 'columns')).toBe(true);
    const { container } = render(
      <BusinessPropertyEditor
        kind="realtime_task"
        fieldKey="checkpoint"
        value={{ offset: 0 }}
        onChange={vi.fn()}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });
});
