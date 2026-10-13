import { useTranslation } from 'react-i18next';
import type { ProductResource } from './resource-types';

export default function AssetSourceStatus({
  record,
  details = false,
}: {
  record: ProductResource;
  details?: boolean;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const state = record.asset_state;
  if (record.kind !== 'asset_entry') return null;
  const label = !state
    ? text('来源状态尚未核对', 'Source state not checked')
    : !state.source_valid
      ? text('来源无效', 'Invalid source')
      : state.stale
        ? text('来源已变化，需重新发布', 'Source changed; republish required')
        : state.publication
          ? text('发布版本与来源一致', 'Published source is current')
          : text('来源有效，尚未发布', 'Valid source; unpublished');
  return (
    <div
      className={
        'mt-2 rounded-md p-2 text-xs ' +
        (state?.source_valid && !state.stale
          ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]'
          : 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)]')
      }
    >
      <p>{label}</p>
      {details && state && (
        <dl className="mt-3 grid gap-3 sm:grid-cols-2">
          <div>
            <dt>{text('当前来源版本', 'Current source version')}</dt>
            <dd>{state.current_source_version ?? '—'}</dd>
          </div>
          <div>
            <dt>{text('发布来源版本', 'Published source version')}</dt>
            <dd>{state.publication?.source_version ?? '—'}</dd>
          </div>
          <div>
            <dt>{text('发布范围', 'Publication scope')}</dt>
            <dd>{state.publication ? text('资产目录定义', 'Asset catalog definition') : '—'}</dd>
          </div>
          <div>
            <dt>{text('发布时间', 'Published at')}</dt>
            <dd>
              {state.publication
                ? new Date(state.publication.published_at).toLocaleString(i18n.language)
                : '—'}
            </dd>
          </div>
          {state.invalid_reason && (
            <div className="sm:col-span-2">
              <dt>{text('来源校验结果', 'Source check')}</dt>
              <dd>{state.invalid_reason}</dd>
            </div>
          )}
        </dl>
      )}
    </div>
  );
}
