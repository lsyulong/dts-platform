import { FormInput } from '../components/ui';
import type { ProductResource } from './resource-types';

import { actionParameters } from './action-models';

export default function ActionParameters({
  record,
  value,
  onChange,
  disabled,
  text,
  inherited = {},
}: {
  record: ProductResource;
  value: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
  disabled: boolean;
  text: (zh: string, en: string) => string;
  inherited?: Record<string, unknown>;
}) {
  const update = (name: string, next: unknown) => {
    const changed = { ...value };
    if (next === undefined) delete changed[name];
    else changed[name] = next;
    onChange(changed);
  };
  return (
    <div className="space-y-4">
      {actionParameters(record).map((parameter) => {
        const required =
          parameter.required && !parameter.has_default && !Object.hasOwn(inherited, parameter.name);
        return (
          <div key={parameter.name} className="space-y-2">
            {!required && (
              <label className="flex items-center gap-2 text-xs text-[var(--color-text-tertiary)]">
                <input
                  type="checkbox"
                  disabled={disabled}
                  checked={Object.hasOwn(value, parameter.name)}
                  onChange={(event) =>
                    update(
                      parameter.name,
                      event.target.checked
                        ? parameter.data_type === 'BOOLEAN'
                          ? false
                          : ''
                        : undefined,
                    )
                  }
                />
                {text('覆盖默认参数', 'Override default')} · {parameter.name}
              </label>
            )}
            {parameter.data_type === 'BOOLEAN' ? (
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={value[parameter.name] === true}
                  disabled={disabled || (!required && !Object.hasOwn(value, parameter.name))}
                  onChange={(event) => update(parameter.name, event.target.checked)}
                />
                {parameter.name} · BOOLEAN
              </label>
            ) : (
              <FormInput
                label={`${parameter.name} · ${parameter.data_type}`}
                type={parameter.data_type === 'DOUBLE' ? 'number' : 'text'}
                value={
                  typeof value[parameter.name] === 'string' ||
                  typeof value[parameter.name] === 'number'
                    ? String(value[parameter.name])
                    : ''
                }
                required={required}
                disabled={disabled || (!required && !Object.hasOwn(value, parameter.name))}
                onChange={(next) =>
                  update(
                    parameter.name,
                    parameter.data_type === 'DOUBLE' && next !== '' ? Number(next) : next,
                  )
                }
              />
            )}
          </div>
        );
      })}
    </div>
  );
}
