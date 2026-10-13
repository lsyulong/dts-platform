import { useId } from 'react';
import { FormInput } from '../components/ui';

export default function MaskingPreviewControls({
  value,
  onChange,
  disabled,
  text,
}: {
  value: unknown;
  onChange: (value: unknown) => void;
  disabled: boolean;
  text: (zh: string, en: string) => string;
}) {
  const id = useId();
  const type =
    value === null
      ? 'null'
      : typeof value === 'number'
        ? 'number'
        : typeof value === 'boolean'
          ? 'boolean'
          : 'text';
  return (
    <div className="space-y-3">
      <label htmlFor={id} className="block text-sm font-medium">
        {text('预览值类型', 'Preview value type')}
      </label>
      <select
        id={id}
        disabled={disabled}
        className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
        value={type}
        onChange={(event) =>
          onChange(
            event.target.value === 'null'
              ? null
              : event.target.value === 'number'
                ? 0
                : event.target.value === 'boolean'
                  ? false
                  : '',
          )
        }
      >
        <option value="text">{text('文本', 'Text')}</option>
        <option value="number">{text('数值', 'Number')}</option>
        <option value="boolean">{text('布尔值', 'Boolean')}</option>
        <option value="null">NULL</option>
      </select>
      {type === 'text' || type === 'number' ? (
        <FormInput
          label={text('预览输入值', 'Preview input value')}
          value={typeof value === 'string' || typeof value === 'number' ? String(value) : ''}
          type={type === 'number' ? 'number' : 'text'}
          required
          disabled={disabled}
          onChange={(next) =>
            onChange(type === 'number' ? (next.trim() ? Number(next) : undefined) : next)
          }
        />
      ) : type === 'boolean' ? (
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={value === true}
            disabled={disabled}
            onChange={(event) => onChange(event.target.checked)}
          />
          {text('值为 true（不勾选为 false）', 'true (unchecked means false)')}
        </label>
      ) : (
        <p className="text-sm">NULL</p>
      )}
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text(
          '使用示例值确认效果。预览不保存输入值；正式查询按已发布规则和用户组应用脱敏。',
          'Use an example value to inspect the transformation. Preview inputs are not stored. Published rules and groups govern actual queries.',
        )}
      </p>
    </div>
  );
}
