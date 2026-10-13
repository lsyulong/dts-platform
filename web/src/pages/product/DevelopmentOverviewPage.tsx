import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { RefreshCw } from 'lucide-react';
import { Button, ErrorBanner, StatusBadge } from '../../components/ui';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import { statusText } from '../../product/ui-text';
import { selectionSessionScope } from '../../product/business-editors/option-sources';
import { useProductJobs, type ProductJob } from './jobs-api';

const sources = [
  { kind: 'sql_document', title: ['SQL 文档', 'SQL documents'], route: '/development/task-runs' },
  {
    kind: 'development_workflow',
    title: ['开发工作流', 'Development workflows'],
    route: '/development/workflow-runs',
  },
] as const;
const statuses = [
  'queued',
  'running',
  'succeeded',
  'failed',
  'cancelled',
  'reconciliation_required',
];

function validPage(
  value: { data: ProductJob[]; total: number } | undefined,
  kind: string,
  status: string,
) {
  return (
    !!value &&
    Number.isSafeInteger(value.total) &&
    value.total >= 0 &&
    Array.isArray(value.data) &&
    value.data.length <= 20 &&
    value.total >= value.data.length &&
    value.data.every(
      (job) =>
        !!job &&
        typeof job === 'object' &&
        job.source_kind === kind &&
        typeof job.id === 'string' &&
        !!job.id &&
        typeof job.source_id === 'string' &&
        !!job.source_id &&
        Number.isSafeInteger(job.source_version) &&
        job.source_version > 0 &&
        statuses.includes(job.status) &&
        (!status || job.status === status) &&
        typeof job.created_at === 'string' &&
        Number.isFinite(Date.parse(job.created_at)),
    ) &&
    new Set(value.data.map((job) => job.id)).size === value.data.length
  );
}

export default function DevelopmentOverviewPage() {
  const session = useAuthStore((state) =>
    JSON.stringify([
      state.user?.tenant_id,
      state.user?.id,
      selectionSessionScope(state.token),
      state.user?.permissions,
    ]),
  );
  return <Overview key={session} />;
}

function Overview() {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const permissions = useAuthStore((state) => state.user?.permissions ?? []);
  const token = useAuthStore((state) => state.token);
  const canRead = !!token && permissions.includes('development:read');
  const [kind, setKind] = useState('all');
  const [status, setStatus] = useState('');
  const sql = useProductJobs('sql_document', status, 0, canRead && kind !== 'development_workflow');
  const workflow = useProductJobs(
    'development_workflow',
    status,
    0,
    canRead && kind !== 'sql_document',
  );
  const selected = sources
    .map((source, index) => ({ source, query: index === 0 ? sql : workflow }))
    .filter(({ source }) => kind === 'all' || kind === source.kind);
  const complete =
    canRead &&
    selected.every(
      ({ source, query }) =>
        !query.error &&
        !query.isPending &&
        !query.isFetching &&
        validPage(query.data, source.kind, status),
    );
  const total = complete ? selected.reduce((sum, { query }) => sum + query.data!.total, 0) : null;
  const rows = complete
    ? selected
        .flatMap(({ query }) => query.data!.data)
        .sort(
          (a, b) =>
            Date.parse(b.created_at) - Date.parse(a.created_at) ||
            (a.id < b.id ? -1 : a.id > b.id ? 1 : 0),
        )
        .slice(0, 20)
    : [];
  const refreshing = selected.some(({ query }) => query.isFetching);

  return (
    <div className="space-y-5">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold">
            {text('数据开发运维概览', 'Development operations overview')}
          </h1>
          <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
            {text(
              '查看 SQL 文档与开发工作流的实际运行。其他领域作业由各模块的运维页面查看。',
              'Inspect actual SQL document and development workflow runs. Other domains have their own operations pages.',
            )}
          </p>
        </div>
        <Button
          variant="outline"
          disabled={!canRead || refreshing}
          onClick={() => {
            for (const { query } of selected) void query.refetch();
          }}
        >
          <RefreshCw size={14} />
          {text('刷新', 'Refresh')}
        </Button>
      </header>
      {!canRead && (
        <ErrorBanner
          message={text('当前账号没有数据开发查看权限', 'Development read permission is required')}
        />
      )}
      <section className="data-panel">
        <div className="data-toolbar flex flex-wrap gap-3">
          <select
            aria-label={text('运行来源', 'Run source')}
            value={kind}
            onChange={(event) => setKind(event.target.value)}
            className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
          >
            <option value="all">{text('全部开发来源', 'All development sources')}</option>
            {sources.map((source) => (
              <option key={source.kind} value={source.kind}>
                {text(source.title[0], source.title[1])}
              </option>
            ))}
          </select>
          <select
            aria-label={text('运行状态', 'Run status')}
            value={status}
            onChange={(event) => setStatus(event.target.value)}
            className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
          >
            <option value="">{text('全部状态', 'All statuses')}</option>
            {statuses.map((value) => (
              <option key={value} value={value}>
                {statusText(value, i18n.language)}
              </option>
            ))}
          </select>
          <span className="self-center text-sm text-[var(--color-text-secondary)]">
            {text('符合筛选的实际实例', 'Matching persisted runs')}：<strong>{total ?? '—'}</strong>
          </span>
        </div>
        <div className="grid gap-3 p-4 md:grid-cols-2">
          {selected.map(({ source, query }) => {
            const known =
              canRead &&
              !query.error &&
              !query.isFetching &&
              validPage(query.data, source.kind, status);
            return (
              <article
                key={source.kind}
                className="rounded-md border border-[var(--color-border-default)] p-4"
              >
                <div className="flex items-center justify-between gap-3">
                  <h2 className="font-medium">{text(source.title[0], source.title[1])}</h2>
                  {canRead && (
                    <Link className="text-sm text-[var(--color-accent)]" to={source.route}>
                      {text('查看实例', 'View runs')}
                    </Link>
                  )}
                </div>
                <p className="mt-2 text-2xl font-semibold">{known ? query.data!.total : '—'}</p>
                {canRead && query.error && (
                  <ErrorBanner
                    message={apiErrorMessage(
                      query.error,
                      text('运行读取失败，数量未知', 'Failed to read runs; count is unknown'),
                    )}
                  />
                )}
                {canRead &&
                  query.data &&
                  !query.error &&
                  !query.isFetching &&
                  !validPage(query.data, source.kind, status) && (
                    <ErrorBanner
                      message={text(
                        '运行响应与实际来源不一致，请刷新后重试',
                        'Run response does not match its source; refresh and retry',
                      )}
                    />
                  )}
              </article>
            );
          })}
        </div>
      </section>
      <section className="data-panel overflow-x-auto">
        <h2 className="p-4 font-medium">{text('最近 20 条匹配实例', 'Latest 20 matching runs')}</h2>
        <table className="w-full text-left text-sm">
          <thead>
            <tr className="border-b border-[var(--color-border-default)] text-[var(--color-text-secondary)]">
              {[
                text('运行实例', 'Run'),
                text('来源 / 版本', 'Source / version'),
                text('状态', 'Status'),
                text('创建时间', 'Created'),
              ].map((label) => (
                <th key={label} className="px-4 py-3">
                  {label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((job) => {
              const source = sources.find((item) => item.kind === job.source_kind)!;
              return (
                <tr key={job.id} className="border-b border-[var(--color-border-default)]">
                  <td className="px-4 py-3">
                    <Link
                      className="text-[var(--color-accent)]"
                      to={`${source.route}?job=${encodeURIComponent(job.id)}`}
                    >
                      {job.id}
                    </Link>
                  </td>
                  <td className="px-4 py-3">
                    {text(source.title[0], source.title[1])} · {job.source_id} · v
                    {job.source_version}
                  </td>
                  <td className="px-4 py-3">
                    <StatusBadge
                      status={job.status}
                      label={statusText(job.status, i18n.language)}
                    />
                  </td>
                  <td className="px-4 py-3">
                    {new Date(job.created_at).toLocaleString(i18n.language)}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {!rows.length && (
          <p className="p-5 text-sm text-[var(--color-text-tertiary)]">
            {complete
              ? text('没有符合筛选条件的运行实例', 'No matching runs')
              : refreshing
                ? text('正在读取实际运行', 'Loading actual runs')
                : text('运行数据暂不可用', 'Run data is currently unavailable')}
          </p>
        )}
      </section>
    </div>
  );
}
