import { useId } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../api/client';
import { useAuthStore } from '../stores/auth';
import { selectionSessionScope } from './business-editors/option-sources';
import type { ProductResource } from './resource-types';
import { ErrorBanner } from '../components/ui';
import { apiErrorMessage } from '../lib/apiError';

interface Props {
  metadataId: string;
  label: string;
  value: unknown;
  onChange: (value: unknown) => void;
  multiple?: boolean;
  timeOnly?: boolean;
  required?: boolean;
  disabled?: boolean;
}
export default function MetadataFieldPicker({
  metadataId,
  label,
  value,
  onChange,
  multiple,
  timeOnly,
  required,
  disabled,
}: Props) {
  const { i18n } = useTranslation(),
    id = useId();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((state) => state.user),
    token = useAuthStore((state) => state.token);
  const source = useQuery({
    queryKey: [
      'metadata-field-picker',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
      metadataId,
    ],
    queryFn: async ({ signal }) => {
      try {
        return (
          await client.get<ProductResource>(
            `/product/resources/metadata_object/${encodeURIComponent(metadataId)}`,
            { signal, headers: { Authorization: `Bearer ${token}` } },
          )
        ).data;
      } catch (error) {
        throw new Error(
          apiErrorMessage(error, text('实际字段读取失败', 'Could not read actual columns')),
        );
      }
    },
    enabled: !!user && !!token && !!metadataId,
  });
  const data = !source.error && source.data?.id === metadataId ? source.data : null;
  const columns = Array.isArray(data?.properties.columns)
    ? data.properties.columns.flatMap((column) =>
        column &&
        typeof column === 'object' &&
        'name' in column &&
        'data_type' in column &&
        typeof column.name === 'string' &&
        typeof column.data_type === 'string' &&
        (!timeOnly || /^(DATE|TIMESTAMP)/i.test(column.data_type))
          ? [{ name: column.name, data_type: column.data_type }]
          : [],
      )
    : [];
  const selected = multiple
    ? Array.isArray(value)
      ? value.filter((item): item is string => typeof item === 'string')
      : []
    : typeof value === 'string'
      ? [value]
      : [];
  const unavailable = selected.filter((name) => !columns.some((column) => column.name === name));
  return (
    <div className="space-y-2">
      <label htmlFor={id} className="block text-sm font-medium text-[var(--color-text-secondary)]">
        {label}
      </label>
      {!metadataId ? (
        <p className="text-xs text-[var(--color-text-tertiary)]">
          {text('请先选择实际元数据对象', 'Select an actual metadata object first')}
        </p>
      ) : source.error ? (
        <ErrorBanner
          message={apiErrorMessage(
            source.error,
            text('实际字段读取失败', 'Could not read actual columns'),
          )}
        />
      ) : (
        <select
          id={id}
          aria-label={label}
          multiple={multiple}
          size={multiple ? Math.min(6, Math.max(3, columns.length)) : undefined}
          value={multiple ? selected : (selected[0] ?? '')}
          required={required}
          disabled={disabled || source.isPending || !columns.length}
          className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
          onChange={(event) =>
            onChange(
              multiple
                ? Array.from(event.currentTarget.selectedOptions, (option) => option.value)
                : event.target.value,
            )
          }
        >
          {!multiple && <option value="">—</option>}
          {unavailable.map((name) => (
            <option key={name} value={name} disabled>
              {name} · {text('当前结构不可用', 'Unavailable in current schema')}
            </option>
          ))}
          {columns.map((column) => (
            <option key={column.name} value={column.name}>
              {column.name} · {column.data_type}
            </option>
          ))}
        </select>
      )}
      {!!unavailable.length && data && (
        <button
          type="button"
          disabled={disabled}
          className="text-xs text-[var(--color-accent)]"
          onClick={() =>
            onChange(multiple ? selected.filter((name) => !unavailable.includes(name)) : '')
          }
        >
          {text('清除已失效字段', 'Clear unavailable columns')}
        </button>
      )}
      {multiple && (
        <p className="text-xs text-[var(--color-text-tertiary)]">
          {text(
            '按住 Ctrl 或 Shift 选择多个实际字段。',
            'Hold Ctrl or Shift to select multiple actual columns.',
          )}
        </p>
      )}
    </div>
  );
}
