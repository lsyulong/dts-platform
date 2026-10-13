import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { RefreshCw, Inbox } from 'lucide-react';
import { Button, ErrorBanner } from '../../components/ui';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import type { ProductModule } from '../../product/catalog-types';
import { useWorkItems, workSources, type WorkSource } from './platform-projection-api';
import { projectionNames, projectionStates } from './projection-copy';

export default function WorkItemsPage(_: { module?: ProductModule } = {}) {
  const identity = useAuthStore((s) =>
    JSON.stringify([s.user?.tenant_id, s.user?.id, s.token, s.user?.permissions]),
  );
  return <WorkItems key={identity} />;
}
function WorkItems() {
  const { i18n } = useTranslation(),
    text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [source, setSource] = useState<WorkSource | undefined>(),
    [offset, setOffset] = useState(0);
  const user = useAuthStore((s) => s.user),
    query = useWorkItems(source, offset);
  const buckets =
    !!user &&
    !!query.data &&
    !query.error &&
    query.data.tenant_id === user.tenant_id &&
    query.data.actor_id === user.id
      ? query.data.sources
      : [];
  const title = (id: string) => {
    const label = projectionNames[id];
    return label ? text(...label) : id;
  };
  const state = (id: string) => {
    const label = projectionStates[id];
    return label ? text(...label) : id;
  };
  const selected = buckets[0],
    total = source && selected?.observation.state === 'known' ? selected.observation.total : null;
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold">{text('我的待办事项', 'My work items')}</h1>
          <p className="mt-2 text-sm text-[var(--color-text-tertiary)]">
            {text(
              '来自真实审批、已入队运行和本人收件箱；在来源页面处理后刷新。',
              'From actual approvals, queued runs and your inbox. Handle items at their source and refresh.',
            )}
          </p>
        </div>
        <Button variant="outline" disabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw size={16} />
          {text('刷新待办', 'Refresh work items')}
        </Button>
      </div>
      <label className="block text-sm">
        {text('待办来源', 'Work-item source')}
        <select
          aria-label={text('待办来源', 'Work-item source')}
          value={source ?? ''}
          onChange={(e) => {
            const choice = workSources.find((v) => v === e.target.value);
            setSource(choice);
            setOffset(0);
          }}
          className="mt-2 rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2"
        >
          <option value="">
            {text('全部来源（每来源最多20条）', 'All sources (up to 20 per source)')}
          </option>
          {workSources.map((s) => (
            <option key={s} value={s}>
              {title(s)}
            </option>
          ))}
        </select>
      </label>
      {query.error && (
        <div role="alert">
          <ErrorBanner
            message={apiErrorMessage(
              query.error,
              text(
                '待办加载失败；当前数量未知',
                'Could not load work items; current counts are unknown',
              ),
            )}
          />
        </div>
      )}
      {query.isPending && (
        <p role="status">{text('正在读取实际来源…', 'Reading actual sources…')}</p>
      )}
      {buckets.map((bucket) => (
        <section
          key={bucket.observation.source}
          className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-5"
        >
          <div className="flex justify-between gap-2">
            <h2 className="font-semibold">{title(bucket.observation.source)}</h2>
            <span>
              {bucket.observation.state === 'known'
                ? `${bucket.observation.total} ${text('条', 'items')}`
                : text('数量未知', 'Count unknown')}
            </span>
          </div>
          <p
            role={bucket.observation.state === 'known' ? undefined : 'status'}
            className="my-2 text-sm text-[var(--color-text-tertiary)]"
          >
            {bucket.observation.state === 'unknown'
              ? text('未知：来源读取失败', 'Unknown: source read failed')
              : state(bucket.observation.state)}
          </p>
          {bucket.observation.state === 'known' && bucket.data.length === 0 && (
            <p className="flex items-center gap-2 py-4 text-sm">
              <Inbox size={18} />
              {text('本页没有待处理记录', 'No work items on this page')}
            </p>
          )}
          {bucket.observation.state === 'known' &&
            bucket.data.map((item) => (
              <article
                key={item.id}
                className="flex flex-wrap items-center justify-between gap-3 border-t border-[var(--color-border-default)] py-4"
              >
                <div>
                  <p className="font-medium">{item.title}</p>
                  <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                    {state(item.status)} · {new Date(item.created_at).toLocaleString(i18n.language)}
                    {item.due_at
                      ? ` · ${text('截止', 'Due')} ${new Date(item.due_at).toLocaleString(i18n.language)}`
                      : ''}
                  </p>
                  <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                    {text('来源记录', 'Source record')} {item.source_id}
                    {item.source_version !== null ? ` · v${item.source_version}` : ''}
                  </p>
                </div>
                {item.route &&
                item.required_permissions.every((p) => user?.permissions.includes(p)) ? (
                  <Link
                    className="rounded-md border border-[var(--color-accent)] px-3 py-2 text-sm text-[var(--color-accent)]"
                    to={item.route}
                  >
                    {text('前往来源处理', 'Open source')}
                  </Link>
                ) : (
                  <span className="text-xs">
                    {text('当前不能访问来源', 'Source access unavailable')}
                  </span>
                )}
              </article>
            ))}
          <details className="mt-3 text-xs text-[var(--color-text-tertiary)]">
            <summary>{text('观测依据', 'Observation provenance')}</summary>
            <p className="mt-2">{bucket.observation.source_table}</p>
            <p className="mt-1 break-words">{bucket.observation.filter}</p>
            <p className="mt-1">
              {text('观测时间', 'Observed at')}:{' '}
              {new Date(bucket.observation.observed_at).toLocaleString(i18n.language)}
            </p>
          </details>
        </section>
      ))}
      {source && (
        <div className="flex items-center justify-between">
          <Button
            variant="outline"
            disabled={!offset || query.isFetching}
            onClick={() => setOffset(Math.max(0, offset - 20))}
          >
            {text('上一页', 'Previous')}
          </Button>
          <span className="text-sm">
            {text('每页20条', '20 per page')} · {Math.floor(offset / 20) + 1}
          </span>
          <Button
            variant="outline"
            disabled={total === null || offset + 20 >= total || query.isFetching}
            onClick={() => setOffset(offset + 20)}
          >
            {text('下一页', 'Next')}
          </Button>
        </div>
      )}
    </div>
  );
}
