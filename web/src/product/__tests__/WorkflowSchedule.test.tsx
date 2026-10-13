import { useState } from 'react';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import ResourceFieldInput from '../ResourceFieldInput';
import { getProductModule } from '../catalog';

const fields = getProductModule('periodic-workflows')!.fields!;

function Schedule({ commit }: { commit: () => void }) {
  const [values, setValues] = useState<Record<string, unknown>>({
    timezone: 'UTC',
    retry_limit: 0,
  });
  return (
    <form
      aria-label="周期计划"
      onSubmit={(event) => {
        event.preventDefault();
        if (event.currentTarget.reportValidity()) commit();
      }}
    >
      {fields
        .filter((field) => ['cron_expression', 'timezone', 'retry_limit'].includes(field.key))
        .map((field) => (
          <ResourceFieldInput
            key={field.key}
            kind="development_workflow"
            field={field}
            value={values[field.key]}
            onChange={(value) => setValues({ ...values, [field.key]: value })}
          />
        ))}
      <button type="submit">保存周期定义</button>
      <output data-testid="schedule-values">{JSON.stringify(values)}</output>
    </form>
  );
}

describe('Periodic workflow schedule fields', () => {
  it('requires Cron and rejects fractional, negative and excessive retries before submit', async () => {
    const user = userEvent.setup();
    const commit = vi.fn();
    render(<Schedule commit={commit} />);
    const form = screen.getByRole('form', { name: '周期计划' });
    const save = screen.getByRole('button', { name: '保存周期定义' });
    await user.click(save);
    expect(commit).not.toHaveBeenCalled();
    await user.type(screen.getByRole('textbox', { name: 'Cron 表达式' }), '0 8 * * MON-FRI');
    const retry = screen.getByLabelText('重试次数');
    for (const value of ['1.5', '-1', '11']) {
      await user.clear(retry);
      await user.type(retry, value);
      expect(form).toBeInvalid();
      await user.click(save);
      expect(commit).not.toHaveBeenCalled();
    }
    await user.clear(retry);
    await user.type(retry, '3');
    expect(form).toBeValid();
    expect(screen.getByTestId('schedule-values')).toHaveTextContent('"retry_limit":3');
    await user.click(save);
    expect(commit).toHaveBeenCalledTimes(1);
  });

  it('shows actual timezone and automatic retry limitations while preserving the definition', async () => {
    render(<Schedule commit={vi.fn()} />);
    expect(screen.getByText(/当前社区调度适配只支持 UTC/)).toBeInTheDocument();
    expect(screen.getByText(/尚未启用周期自动重试/)).toBeInTheDocument();
    expect(screen.getByText(/星期 0\/7 表示周日/)).toBeInTheDocument();
    const timezone = screen.getByRole('textbox', { name: '时区' });
    await userEvent.clear(timezone);
    await userEvent.type(timezone, 'Asia/Shanghai');
    expect(timezone).toHaveValue('Asia/Shanghai');
  });
});
