import { useDeferredValue, useId, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useProductResources } from './api';
import { ErrorBanner } from '../components/ui';
import { apiErrorMessage } from '../lib/apiError';
import { productText } from './ui-text';
import client from '../api/client';
import { useAuthStore } from '../stores/auth';
import UserReferencePicker from './UserReferencePicker';

interface Props {
  kind: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  required?: boolean;
  excludeId?: string;
  resourceKind?: string;
}

export default function ReferencePicker(props: Props) {
  if (props.kind === 'user') return <UserReferencePicker {...props} />;
  return <ResourceReferencePicker {...props} />;
}

function ResourceReferencePicker({
  kind,
  label,
  value,
  onChange,
  disabled,
  required,
  excludeId,
}: Props) {
  const { i18n } = useTranslation();
  const text = productText(i18n.language);
  const id = useId();
  const [search, setSearch] = useState('');
  const q = useDeferredValue(search);
  const user = useAuthStore((state) => state.user);
  const records = useProductResources(kind, { q, limit: 100, offset: 0 }, kind !== 'datasource');
  const datasources = useQuery({
    queryKey: ['product-source-reference', user?.tenant_id, user?.id],
    enabled: !!user && kind === 'datasource',
    queryFn: ({ signal }) =>
      client
        .get<{ data: { id: string; name: string; db_type: string }[] }>('/datasources', { signal })
        .then((response) =>
          response.data.data.map((source) => ({
            id: source.id,
            name: source.name,
            code: source.db_type,
          })),
        ),
  });
  const isPending = kind === 'datasource' ? datasources.isPending : records.isPending;
  const error = kind === 'datasource' ? datasources.error : records.error;
  const options =
    (kind === 'datasource'
      ? datasources.data?.filter((source) =>
          `${source.name} ${source.code}`.toLocaleLowerCase().includes(q.toLocaleLowerCase()),
        )
      : records.data?.data
    )?.filter((record) => record.id !== excludeId) ?? [];

  return (
    <div className="space-y-2">
      <label htmlFor={id} className="block text-sm font-medium text-[var(--color-text-secondary)]">
        {label}
        {required && <span className="ml-1 text-[var(--color-error)]">*</span>}
      </label>
      <input
        aria-label={`${label} — ${text.referenceSearch}`}
        placeholder={text.referenceSearch}
        value={search}
        onChange={(event) => setSearch(event.target.value)}
        disabled={disabled}
        className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm"
      />
      <select
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        disabled={disabled || isPending}
        required={required}
        className="w-full rounded-md border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm"
      >
        <option value="">{isPending ? text.loading : text.choose}</option>
        {value && !options.some((record) => record.id === value) && (
          <option value={value}>{value}</option>
        )}
        {options.map((record) => (
          <option key={record.id} value={record.id}>
            {record.name} · {record.code}
          </option>
        ))}
      </select>
      {error && <ErrorBanner message={apiErrorMessage(error, text.error)} />}
    </div>
  );
}
