import { useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../components/ui/dialog';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';
import type { ProductModule } from '../../product/catalog-types';
import ProductModulePage from '../../product/ProductModulePage';
import { statusText } from '../../product/ui-text';
import JobNodeResult from './JobNodeResult';
import type { ProductJob } from './jobs-api';

const metricKinds = ['atomic_metric', 'derived_metric', 'composite_metric'] as const;
const runStatuses = [
  'queued',
  'running',
  'succeeded',
  'failed',
  'cancelled',
  'reconciliation_required',
];

export default function MetricsPage({ module }: { module: ProductModule }) {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <MetricTabs key={`${identity}:${module.resourceKind}`} module={module} scope={identity} />;
}
function MetricTabs({ module, scope }: { module: ProductModule; scope: string }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [tab, setTab] = useState<'definitions' | 'runs'>('definitions');
  const [params, setParams] = useSearchParams();
  const active = params.has('job') ? 'runs' : tab;
  const kind = metricKinds.find((kind) => kind === module.resourceKind);
  if (!kind) return <ProductModulePage module={module} />;
  return (
    <div className="space-y-5">
      <div
        role="tablist"
        aria-label={text('指标管理', 'Metric management')}
        className="flex gap-4 border-b border-[var(--color-border-default)]"
      >
        {(['definitions', 'runs'] as const).map((value) => (
          <button
            key={value}
            role="tab"
            aria-selected={active === value}
            onClick={() => {
              setTab(value);
              if (params.has('job')) {
                const next = new URLSearchParams(params);
                next.delete('job');
                setParams(next);
              }
            }}
            className={`border-b-2 px-3 py-3 text-sm ${active === value ? 'border-[var(--color-accent)] text-[var(--color-accent)]' : 'border-transparent text-[var(--color-text-secondary)]'}`}
          >
            {value === 'definitions'
              ? text('指标定义', 'Metric definitions')
              : text('实际计算记录', 'Observed calculation runs')}
          </button>
        ))}
      </div>
      {active === 'definitions' ? (
        <ProductModulePage module={module} />
      ) : (
        <MetricRuns initialKind={kind} scope={scope} />
      )}
    </div>
  );
}
function MetricRuns({
  initialKind,
  scope,
}: {
  initialKind: (typeof metricKinds)[number];
  scope: string;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const token = useAuthStore((state) => state.token);
  const canRead = useAuthStore((state) => !!state.user?.permissions.includes('metrics:read'));
  const canExecute = useAuthStore((state) => !!state.user?.permissions.includes('metrics:execute'));
  const [kind, setKind] = useState<string>(initialKind);
  const [status, setStatus] = useState('');
  const [offset, setOffset] = useState(0);
  const [params, setParams] = useSearchParams();
  const selected = params.get('job') ?? undefined;
  const setSelected = (id: string | undefined) =>
    setParams((current) => {
      const next = new URLSearchParams(current);
      if (id) next.set('job', id);
      else next.delete('job');
      return next;
    });
  const [pending, setPending] = useState<{
    job: ProductJob;
    action: 'cancel' | 'retry';
    key: string;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const keys = useRef(new Map<string, string>());
  const cache = useQueryClient();
  const runs = useQuery({
    queryKey: ['metric-jobs', scope, kind, status, offset],
    queryFn: ({ signal }) =>
      client
        .get<{ data: ProductJob[]; total: number }>('/product/jobs', {
          params: { source_kind: kind, status: status || undefined, limit: 20, offset },
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((response) => response.data),
    enabled: !!token && canRead,
    refetchInterval: (query) =>
      query.state.data?.data.some((job) => ['queued', 'running'].includes(job.status))
        ? 5000
        : false,
  });
  const detail = useQuery({
    queryKey: ['metric-job-detail', scope, selected],
    queryFn: ({ signal }) =>
      client
        .get<ProductJob>(`/product/jobs/${encodeURIComponent(selected ?? '')}`, {
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((response) => response.data),
    enabled: !!token && canRead && !!selected,
    staleTime: 0,
    refetchInterval: (query) =>
      ['queued', 'running'].includes(query.state.data?.status ?? '') ? 5000 : false,
  });
  const visibleDetail =
    !detail.isFetching &&
    !detail.isError &&
    detail.data?.id === selected &&
    metricKinds.some((kind) => kind === detail.data?.source_kind)
      ? detail.data
      : undefined;
  const actOnRun = async () => {
    if (!pending || busy || !canExecute) return;
    const session = captureSession();
    setBusy(true);
    setError('');
    try {
      await client.post(
        `/product/jobs/${encodeURIComponent(pending.job.id)}/${pending.action}`,
        pending.action === 'retry' ? { idempotency_key: pending.key } : {},
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (isCurrentSession(session)) {
        if (pending.action === 'retry') keys.current.delete(pending.job.id);
        setPending(null);
        await cache.invalidateQueries({ queryKey: ['metric-jobs', scope] });
        await cache.invalidateQueries({ queryKey: ['metric-job-detail', scope] });
      }
    } catch (caught) {
      if (isCurrentSession(session))
        setError(
          apiErrorMessage(caught, text('计算运行操作失败', 'Calculation run operation failed')),
        );
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  const labelKind = (value: string) =>
    value === 'atomic_metric'
      ? text('原子指标', 'Atomic metrics')
      : value === 'derived_metric'
        ? text('派生指标', 'Derived metrics')
        : text('复合指标', 'Composite metrics');
  return (
    <div className="space-y-4">
      <p className="text-sm text-[var(--color-text-secondary)]">
        {text(
          '这里只展示真实持久化任务。发布定义不会生成计算结果；结果查看会重新核验当前授权、模型及元数据版本。',
          'Only persisted jobs appear here. Publishing creates no calculated data; viewing results rechecks current grants, models and metadata.',
        )}
      </p>
      {!canRead && (
        <ErrorBanner
          message={text('当前账号没有指标读取权限', 'Your account lacks metrics read permission')}
        />
      )}
      <section className="overflow-hidden rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)]">
        <div className="flex flex-wrap items-end gap-3 p-4">
          <label className="text-sm">
            <span className="mb-1 block">{text('指标类型', 'Metric type')}</span>
            <select
              aria-label={text('指标类型', 'Metric type')}
              value={kind}
              onChange={(event) => {
                setKind(event.target.value);
                setOffset(0);
                setSelected(undefined);
              }}
              className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2"
            >
              {metricKinds.map((value) => (
                <option key={value} value={value}>
                  {labelKind(value)}
                </option>
              ))}
            </select>
          </label>
          <label className="text-sm">
            <span className="mb-1 block">{text('运行状态', 'Run status')}</span>
            <select
              aria-label={text('运行状态', 'Run status')}
              value={status}
              onChange={(event) => {
                setStatus(event.target.value);
                setOffset(0);
                setSelected(undefined);
              }}
              className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2"
            >
              <option value="">{text('全部状态', 'All statuses')}</option>
              {runStatuses.map((value) => (
                <option key={value} value={value}>
                  {statusText(value, i18n.language)}
                </option>
              ))}
            </select>
          </label>
          <Button
            variant="outline"
            disabled={!canRead || runs.isFetching}
            onClick={() => void runs.refetch()}
          >
            {text('刷新记录', 'Refresh runs')}
          </Button>
        </div>
        {runs.error && (
          <div className="p-4">
            <ErrorBanner
              message={apiErrorMessage(
                runs.error,
                text('指标计算记录加载失败', 'Could not load metric runs'),
              )}
            />
          </div>
        )}
        <div className="overflow-auto">
          <table className="w-full text-left text-sm">
            <thead className="bg-[var(--color-bg-secondary)] text-[var(--color-text-tertiary)]">
              <tr>
                {[
                  text('运行 / 来源版本', 'Run / source version'),
                  text('实际状态', 'Observed status'),
                  text('创建时间', 'Created'),
                  text('操作', 'Actions'),
                ].map((label) => (
                  <th key={label} className="px-4 py-3 font-medium">
                    {label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {canRead &&
                runs.data?.data.map((job) => (
                  <tr key={job.id} className="border-t border-[var(--color-border-default)]">
                    <td className="px-4 py-3">
                      <button
                        className="text-[var(--color-accent)] hover:underline"
                        onClick={() => setSelected(job.id)}
                      >
                        {job.id}
                      </button>
                      <p className="mt-1 text-xs">
                        {job.source_id} · v{job.source_version}
                      </p>
                    </td>
                    <td className="px-4 py-3">
                      {statusText(job.status, i18n.language)}
                      {job.cancel_requested && (
                        <p className="text-xs">
                          {text(
                            '已请求取消，等待执行器确认',
                            'Cancellation requested; awaiting the executor',
                          )}
                        </p>
                      )}
                    </td>
                    <td className="px-4 py-3">
                      {new Date(job.created_at).toLocaleString(i18n.language)}
                    </td>
                    <td className="px-4 py-3">
                      {canExecute && ['queued', 'running'].includes(job.status) && (
                        <Button
                          variant="ghost"
                          disabled={job.cancel_requested}
                          onClick={() => {
                            setPending({ job, action: 'cancel', key: '' });
                            setError('');
                          }}
                        >
                          {text('取消运行', 'Cancel run')}
                        </Button>
                      )}
                      {canExecute &&
                        job.can_retry &&
                        ['failed', 'cancelled'].includes(job.status) && (
                          <Button
                            variant="ghost"
                            onClick={() => {
                              const key = keys.current.get(job.id) ?? crypto.randomUUID();
                              keys.current.set(job.id, key);
                              setPending({ job, action: 'retry', key });
                              setError('');
                            }}
                          >
                            {text('重试计算', 'Retry calculation')}
                          </Button>
                        )}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
        {canRead && !runs.error && !runs.data?.data.length && (
          <p className="p-10 text-center text-sm text-[var(--color-text-tertiary)]">
            {runs.isPending
              ? text('正在加载真实任务', 'Loading persisted jobs')
              : text(
                  '暂无符合条件的实际计算记录',
                  'No persisted calculation runs match this filter',
                )}
          </p>
        )}
        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-[var(--color-border-default)] p-4 text-sm">
          <span>{canRead && runs.data ? `${runs.data.total} ${text('次运行', 'runs')}` : '—'}</span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={!canRead || !offset || runs.isFetching}
              onClick={() => setOffset(Math.max(0, offset - 20))}
            >
              {text('上一页', 'Previous')}
            </Button>
            <Button
              variant="outline"
              disabled={!canRead || !runs.data || offset + 20 >= runs.data.total || runs.isFetching}
              onClick={() => setOffset(offset + 20)}
            >
              {text('下一页', 'Next')}
            </Button>
          </div>
        </div>
      </section>
      <Dialog
        open={canRead && !!selected}
        onOpenChange={(open) => {
          if (!open) setSelected(undefined);
        }}
      >
        <DialogContent className="max-w-4xl">
          <DialogTitle>{text('实际指标运行详情', 'Observed metric run details')}</DialogTitle>
          <DialogDescription>
            {selected} ·{' '}
            {detail.data
              ? statusText(detail.data.status, i18n.language)
              : text('正在核验结果读取权限', 'Checking result access')}
          </DialogDescription>
          {detail.error && (
            <ErrorBanner
              message={apiErrorMessage(
                detail.error,
                text('运行详情加载失败', 'Could not load run details'),
              )}
            />
          )}
          {detail.isFetching && (
            <p className="text-sm">
              {text('正在重新核验结果读取权限', 'Rechecking current result access')}
            </p>
          )}
          {visibleDetail?.error && <ErrorBanner message={visibleDetail.error} />}
          <div className="max-h-[65vh] space-y-3 overflow-auto">
            {visibleDetail?.nodes.map((node) => (
              <section
                key={node.id}
                className="rounded-md border border-[var(--color-border-default)] p-4"
              >
                <strong className="text-sm">
                  {node.name} · {statusText(node.status, i18n.language)}
                </strong>
                {node.error && <ErrorBanner message={node.error} />}
                {node.result_access === 'restricted' && (
                  <p className="mt-2 text-sm text-[var(--color-text-secondary)]">
                    {text(
                      '当前授权、发布依赖或物理元数据证明已失效，不能查看该计算结果。',
                      'Current grants, publication dependencies or physical metadata proof no longer allow this result.',
                    )}
                  </p>
                )}
                {node.result_access === 'authorized' && node.result !== null && (
                  <JobNodeResult result={node.result} />
                )}
                {node.result === null && node.result_access !== 'restricted' && (
                  <p className="mt-2 text-sm text-[var(--color-text-tertiary)]">
                    {text(
                      '此节点暂无可查看的实际计算结果',
                      'No observed calculation result is available for this node',
                    )}
                  </p>
                )}
              </section>
            ))}
          </div>
        </DialogContent>
      </Dialog>
      <Dialog
        open={canExecute && !!pending}
        onOpenChange={(open) => {
          if (!open && !busy) setPending(null);
        }}
      >
        <DialogContent>
          <DialogTitle>
            {pending?.action === 'cancel'
              ? text('取消实际计算', 'Cancel calculation')
              : text('重试只读计算', 'Retry read-only calculation')}
          </DialogTitle>
          <DialogDescription>
            {pending?.job.id} ·{' '}
            {text(
              '服务端会再次核验当前发布版本、依赖和授权。失败重试保留同一请求标识。',
              'The server rechecks publication versions, dependencies and grants. Failed retries keep the same request key.',
            )}
          </DialogDescription>
          {error && <ErrorBanner message={error} />}
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="outline" disabled={busy} onClick={() => setPending(null)}>
              {text('关闭', 'Close')}
            </Button>
            <Button disabled={busy || !canExecute} onClick={() => void actOnRun()}>
              {busy ? text('正在提交', 'Submitting') : text('确认操作', 'Confirm operation')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
