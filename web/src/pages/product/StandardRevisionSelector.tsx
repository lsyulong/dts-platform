import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useProductResources } from '../../product/api';
import { Button, ErrorBanner } from '../../components/ui';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import {
  useStandardRevisions,
  type StandardKind,
  type StandardVersionRequest,
} from './standards-api';

const input =
  'rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm';
export default function StandardRevisionSelector({
  kind,
  label,
  value,
  onChange,
  disabled,
  objectType,
}: {
  kind: StandardKind;
  label: string;
  value: StandardVersionRequest | null;
  onChange: (value: StandardVersionRequest | null) => void;
  disabled?: boolean;
  objectType?: 'table' | 'column' | 'metric';
}) {
  const { i18n } = useTranslation(),
    text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((s) => s.user),
    canRead = !!user?.permissions.includes('standards:read');
  const [offset, setOffset] = useState(0),
    [versionOffset, setVersionOffset] = useState(0);
  const list = useProductResources(kind, { limit: 20, offset }, canRead);
  const revisions = useStandardRevisions(kind, value?.id, versionOffset);
  const choices = !list.error
    ? (list.data?.data ?? []).filter(
        (r) =>
          r.tenant_id === user?.tenant_id &&
          r.kind === kind &&
          (!objectType || r.properties.object_type === objectType),
      )
    : [];
  const releases =
    !list.error && !revisions.error
      ? (revisions.data?.data ?? []).filter(
          (r) => r.state === 'active' && (!objectType || r.properties.object_type === objectType),
        )
      : [];
  const selectedKnown = releases.some((r) => r.revision_version === value?.version);
  return (
    <div className="space-y-2 rounded-md border border-[var(--color-border-default)] p-3">
      <label className="block text-xs font-medium">
        {label}
        <select
          aria-label={`${label} ${text('定义', 'definition')}`}
          className={`${input} mt-2 w-full`}
          disabled={disabled || !canRead || !!list.error}
          value={value?.id ?? ''}
          onChange={(e) => {
            setVersionOffset(0);
            onChange(e.target.value ? { id: e.target.value, version: 0 } : null);
          }}
        >
          <option value="">{text('不绑定', 'No binding')}</option>
          {value?.id && !choices.some((r) => r.id === value.id) && (
            <option value={value.id}>
              {value.id} {text('当前绑定，需复核', 'Current binding; recheck required')}
            </option>
          )}
          {choices.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name} · {r.code}
            </option>
          ))}
        </select>
      </label>
      {value && (
        <label className="block text-xs">
          {text('已发布版本', 'Published revision')}
          <select
            aria-label={`${label} ${text('版本', 'revision')}`}
            className={`${input} mt-2 w-full`}
            disabled={
              disabled || !canRead || !!list.error || !!revisions.error || revisions.isFetching
            }
            value={value.version || ''}
            onChange={(e) => onChange({ id: value.id, version: Number(e.target.value) })}
          >
            <option value="">
              {text('选择有效发布版本', 'Select an active published revision')}
            </option>
            {value.version > 0 && !selectedKnown && (
              <option value={value.version} disabled>
                {text('当前版本待复核/已撤回', 'Current revision unverified / withdrawn')} ·{' '}
                {value.version}
              </option>
            )}
            {releases.map((r) => (
              <option key={r.revision_version} value={r.revision_version}>
                v{r.revision_version} · {r.definition_hash.slice(0, 12)}
              </option>
            ))}
          </select>
        </label>
      )}
      {(list.error || revisions.error) && (
        <ErrorBanner
          message={apiErrorMessage(
            list.error || revisions.error,
            text(
              '标准版本读取失败；不能改用草稿',
              'Could not read revisions; a draft cannot substitute for a release',
            ),
          )}
        />
      )}
      <div className="flex flex-wrap gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={disabled || offset === 0 || list.isFetching}
          onClick={() => setOffset(Math.max(0, offset - 20))}
          aria-label={`${label} ${text('上一页定义', 'Previous definitions')}`}
        >
          ‹
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={
            disabled || !!list.error || offset + 20 >= (list.data?.total ?? 0) || list.isFetching
          }
          onClick={() => setOffset(offset + 20)}
          aria-label={`${label} ${text('下一页定义', 'Next definitions')}`}
        >
          ›
        </Button>
        {value && (
          <>
            <Button
              variant="outline"
              size="sm"
              disabled={disabled || versionOffset === 0 || revisions.isFetching}
              onClick={() => setVersionOffset(Math.max(0, versionOffset - 20))}
              aria-label={`${label} ${text('上一页版本', 'Previous revisions')}`}
            >
              {text('旧页', 'Previous')}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={
                disabled ||
                !!revisions.error ||
                versionOffset + 20 >= (revisions.data?.total ?? 0) ||
                revisions.isFetching
              }
              onClick={() => setVersionOffset(versionOffset + 20)}
              aria-label={`${label} ${text('下一页版本', 'Next revisions')}`}
            >
              {text('更多版本', 'More revisions')}
            </Button>
          </>
        )}
      </div>
      {value && revisions.data && !revisions.error && releases.length === 0 && (
        <p className="text-xs text-[var(--color-text-tertiary)]">
          {text(
            '本页没有有效发布版本。草稿不能用于模型发布。',
            'This page has no active published revisions. Drafts cannot be used for model publication.',
          )}
        </p>
      )}
    </div>
  );
}
