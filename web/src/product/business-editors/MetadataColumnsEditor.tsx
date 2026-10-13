import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import { useAuthStore } from '../../stores/auth';
import type { ProductResource } from '../resource-types';
import type { BusinessPropertyEditorProps } from './types';
import { isRecord } from './business-models';

export default function MetadataColumnsEditor({
  kind,
  value,
  properties,
  onChange,
  disabled,
}: BusinessPropertyEditorProps) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  const objectId = properties?.[kind === 'quality_object' ? 'metadata_object_id' : 'object_id'];
  const metadata = useQuery({
    queryKey: ['metadata-column-picker', user?.tenant_id, user?.id, objectId],
    queryFn: ({ signal }) =>
      client
        .get<ProductResource>(
          `/product/resources/metadata_object/${encodeURIComponent(String(objectId))}`,
          { signal, headers: { Authorization: `Bearer ${token}` } },
        )
        .then((response) => {
          if (
            response.data.id !== objectId ||
            response.data.tenant_id !== user?.tenant_id ||
            response.data.kind !== 'metadata_object'
          )
            throw new Error(text('元数据身份不匹配', 'Metadata identity mismatch'));
          return response.data;
        }),
    enabled: !!user && !!token && typeof objectId === 'string' && !!objectId,
  });
  const selected =
    Array.isArray(value) && value.every((column) => typeof column === 'string') ? value : [];
  const columns = Array.isArray(metadata.data?.properties.columns)
    ? metadata.data.properties.columns
        .filter(isRecord)
        .flatMap((column) =>
          typeof column.name === 'string'
            ? [{ name: column.name, type: String(column.data_type ?? '') }]
            : [],
        )
    : [];
  const missing = selected.filter((column) => !columns.some((actual) => actual.name === column));
  if (!objectId)
    return (
      <p className="text-sm text-[var(--color-text-tertiary)]">
        {text(
          '先选择真实元数据对象，再选择字段',
          'Select a metadata object before choosing columns',
        )}
      </p>
    );
  if (metadata.error)
    return (
      <ErrorBanner
        message={apiErrorMessage(metadata.error, text('字段加载失败', 'Could not load columns'))}
      />
    );
  if (!metadata.data)
    return <p className="text-sm">{text('正在读取采集字段', 'Loading collected columns')}</p>;
  if (metadata.data.status !== 'published')
    return (
      <ErrorBanner
        message={text(
          '该对象已在源端缺失，请重新采集并选择有效对象',
          'The object is missing at its source. Recollect and select an active object.',
        )}
      />
    );
  return (
    <div className="space-y-3">
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text(
          '不单独选择字段时使用全部采集字段；提交时保存实际字段快照',
          'Leaving the selection empty uses all collected columns; submission freezes the actual columns',
        )}
      </p>
      {!!missing.length && (
        <ErrorBanner
          message={`${text('原选择中存在已移除字段', 'Selected columns no longer exist')}: ${missing.join(', ')}`}
        />
      )}
      <div className="max-h-64 space-y-2 overflow-auto rounded border border-[var(--color-border-default)] p-3">
        {columns.map((column) => (
          <label key={column.name} className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={selected.includes(column.name)}
              disabled={disabled}
              onChange={(event) =>
                onChange(
                  event.target.checked
                    ? [...selected, column.name]
                    : selected.filter((name) => name !== column.name),
                )
              }
            />
            <span>{column.name}</span>
            <span className="ml-auto text-xs text-[var(--color-text-tertiary)]">{column.type}</span>
          </label>
        ))}
      </div>
      {!!selected.length && (
        <button
          type="button"
          disabled={disabled}
          className="text-xs text-[var(--color-accent)]"
          onClick={() => onChange([])}
        >
          {text('清除选择，使用全部字段', 'Clear selection and use all columns')}
        </button>
      )}
    </div>
  );
}
