import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../api/client';
import { ErrorBanner } from '../components/ui';
import { useAuthStore } from '../stores/auth';
import { apiErrorMessage } from '../lib/apiError';
import { selectionSessionScope } from './business-editors/option-sources';
import type { ProductResource } from './resource-types';
import MetadataFieldPicker from './MetadataFieldPicker';

export default function DerivedMetricFieldPicker({
  atomicId,
  label,
  value,
  onChange,
  disabled,
}: {
  atomicId: string;
  label: string;
  value: unknown;
  onChange: (value: unknown) => void;
  disabled?: boolean;
}) {
  const { i18n } = useTranslation();
  const zh = i18n.language.startsWith('zh');
  const user = useAuthStore((state) => state.user),
    token = useAuthStore((state) => state.token);
  const source = useQuery({
    queryKey: [
      'derived-metric-fields',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
      atomicId,
    ],
    queryFn: async ({ signal }) =>
      (
        await client.get<ProductResource>(
          `/product/resources/atomic_metric/${encodeURIComponent(atomicId)}`,
          { signal, headers: { Authorization: `Bearer ${token}` } },
        )
      ).data,
    enabled: !!user && !!token && !!atomicId,
  });
  if (!atomicId)
    return (
      <p className="text-sm text-[var(--color-text-tertiary)]">
        {zh
          ? '请先选择原子指标，再选择实际维度。'
          : 'Select an atomic metric to choose actual dimensions.'}
      </p>
    );
  if (source.error)
    return (
      <ErrorBanner
        message={apiErrorMessage(
          source.error,
          zh ? '原子指标读取失败' : 'Could not read atomic metric',
        )}
      />
    );
  const actual =
    source.data?.id === atomicId && source.data.kind === 'atomic_metric' ? source.data : null;
  return (
    <MetadataFieldPicker
      metadataId={
        typeof actual?.properties.metadata_object_id === 'string'
          ? actual.properties.metadata_object_id
          : ''
      }
      label={label}
      value={value}
      onChange={onChange}
      multiple
      disabled={disabled || source.isPending}
    />
  );
}
