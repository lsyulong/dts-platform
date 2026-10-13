import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { RefreshCw } from 'lucide-react';
import { Button, ErrorBanner } from '../../components/ui';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';
import type { ProductModule } from '../../product/catalog-types';
import { usePlatformStatistics, type StatisticsView } from './platform-projection-api';
import { projectionNames, projectionStates } from './projection-copy';

export default function PlatformStatisticsPage({
  module,
  view,
}: { module?: ProductModule; view?: StatisticsView } = {}) {
  const chosen = view ?? (module?.resourceKind === 'usage_statistics' ? 'usage' : 'system');
  const identity = useAuthStore((s) =>
    JSON.stringify([s.user?.tenant_id, s.user?.id, s.token, s.user?.permissions]),
  );
  return <Statistics key={`${identity}:${chosen}`} view={chosen} />;
}
function initialWindow() {
  const end = new Date(),
    start = new Date(end.getTime() - 7 * 86400000);
  return { start: start.toISOString().slice(0, 16), end: end.toISOString().slice(0, 16) };
}
function utc(value: string) {
  return value ? `${value}:00Z` : '';
}
function Statistics({ view }: { view: StatisticsView }) {
  const { i18n } = useTranslation(),
    text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((s) => s.user),
    canRead = view === 'usage' || !!user?.permissions.includes('system:read');
  const [inputs, setInputs] = useState(initialWindow),
    [appliedWindow, setWindow] = useState(() => ({
      start: utc(inputs.start),
      end: utc(inputs.end),
    })),
    [error, setError] = useState('');
  const query = usePlatformStatistics(view, appliedWindow.start, appliedWindow.end);
  const buckets =
    !!user &&
    !!query.data &&
    canRead &&
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
  const dimension = (value: string, source: string) =>
    value
      .split(':')
      .map((v) =>
        v === 'pending' && source === 'service_calls'
          ? text('已预留待完成', 'Reserved, unfinished')
          : state(v),
      )
      .join(' / ');
  const apply = () => {
    const start = Date.parse(utc(inputs.start)),
      end = Date.parse(utc(inputs.end));
    if (
      !Number.isFinite(start) ||
      !Number.isFinite(end) ||
      end <= start ||
      end - start > 366 * 86400000 ||
      end > Date.now() + 300000
    ) {
      setError(
        text(
          '请选择正数且最多366天的UTC时间窗口，结束时间不能在未来。',
          'Choose a positive UTC window of at most 366 days. The end cannot be in the future.',
        ),
      );
      return;
    }
    setError('');
    setWindow({ start: new Date(start).toISOString(), end: new Date(end).toISOString() });
  };
  const exportJson = async () => {
    const session = captureSession(),
      permissions = JSON.stringify(user?.permissions);
    setError('');
    const result = await query.refetch();
    if (
      !isCurrentSession(session) ||
      JSON.stringify(useAuthStore.getState().user?.permissions) !== permissions
    )
      return;
    if (result.error || !result.data) {
      setError(
        text(
          '导出前读取失败，未导出旧缓存统计。',
          'The export read failed. Cached statistics were not exported.',
        ),
      );
      return;
    }
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(result.data, null, 2)], { type: 'application/json;charset=utf-8' }),
    );
    const link = document.createElement('a');
    link.href = url;
    link.download = `platform-${view}-statistics.json`;
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold">
            {view === 'system'
              ? text('系统统计', 'System statistics')
              : text('我的使用统计', 'My usage statistics')}
          </h1>
          <p className="mt-2 text-sm text-[var(--color-text-tertiary)]">
            {text(
              '按持久化记录统计。定义、同步运行和产品运行分别展示；不估算外部引擎状态。',
              'Counts from persisted records. Definitions, pipeline runs and product executions are separate; external engine state is not estimated.',
            )}
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            disabled={!canRead || query.isFetching}
            onClick={() => void query.refetch()}
          >
            <RefreshCw size={16} />
            {text('刷新统计', 'Refresh statistics')}
          </Button>
          <Button
            variant="outline"
            disabled={!canRead || query.isFetching}
            onClick={() => void exportJson()}
          >
            {text('导出实际JSON统计', 'Export observed JSON statistics')}
          </Button>
        </div>
      </div>
      {!canRead && (
        <div role="alert">
          <ErrorBanner
            message={text('当前无权读取系统统计。', 'You cannot currently read system statistics.')}
          />
        </div>
      )}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          apply();
        }}
        className="flex flex-wrap items-end gap-3"
      >
        <label className="text-sm">
          {text('开始时间（UTC，包含）', 'Start (UTC, inclusive)')}
          <input
            type="datetime-local"
            value={inputs.start}
            onChange={(e) => setInputs({ ...inputs, start: e.target.value })}
            className="mt-2 block rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2"
          />
        </label>
        <label className="text-sm">
          {text('结束时间（UTC，不包含）', 'End (UTC, exclusive)')}
          <input
            type="datetime-local"
            value={inputs.end}
            onChange={(e) => setInputs({ ...inputs, end: e.target.value })}
            className="mt-2 block rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2"
          />
        </label>
        <Button disabled={!canRead || query.isFetching} type="submit">
          {text('应用时间窗口', 'Apply time window')}
        </Button>
      </form>
      {(error || query.error) && (
        <div role="alert">
          <ErrorBanner
            message={
              error ||
              apiErrorMessage(
                query.error,
                text('统计加载失败；数量未知', 'Could not read statistics; counts are unknown'),
              )
            }
          />
        </div>
      )}
      {query.isPending && canRead && (
        <p role="status">{text('正在读取实际统计…', 'Reading actual statistics…')}</p>
      )}
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text('当前已应用窗口', 'Applied window')}: [{appliedWindow.start}, {appliedWindow.end})
      </p>
      <div className="grid gap-4 lg:grid-cols-2">
        {buckets.map((bucket) => (
          <section
            key={bucket.observation.source}
            className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-5"
          >
            <div className="flex justify-between gap-3">
              <h2 className="font-semibold">{title(bucket.observation.source)}</h2>
              <span className="text-xl font-semibold text-[var(--color-accent)]">
                {bucket.observation.state === 'known'
                  ? bucket.observation.total
                  : text('未知', 'Unknown')}
              </span>
            </div>
            <p className="my-2 text-xs text-[var(--color-text-tertiary)]">
              {bucket.observation.state === 'unknown'
                ? text('未知：来源读取失败', 'Unknown: source read failed')
                : state(bucket.observation.state)}
            </p>
            {bucket.observation.state === 'known' && bucket.data.length === 0 && (
              <p className="py-3 text-sm">
                {text('该窗口没有已记录事件', 'No recorded rows in this window')}
              </p>
            )}
            {bucket.observation.state === 'known' && (
              <dl className="space-y-2">
                {bucket.data.map((row) => (
                  <div
                    key={row.dimension}
                    className="flex justify-between gap-3 border-t border-[var(--color-border-default)] pt-2 text-sm"
                  >
                    <dt className="break-all">
                      {dimension(row.dimension, bucket.observation.source)}
                    </dt>
                    <dd>{row.count.toLocaleString(i18n.language)}</dd>
                  </div>
                ))}
              </dl>
            )}
            <details className="mt-4 text-xs text-[var(--color-text-tertiary)]">
              <summary>
                {text('来源、过滤与观测时间', 'Source, filter and observation time')}
              </summary>
              <p className="mt-2">{bucket.observation.source_table}</p>
              <p className="mt-1 break-words">{bucket.observation.filter}</p>
              <p className="mt-1">
                {new Date(bucket.observation.observed_at).toLocaleString(i18n.language)}
              </p>
            </details>
          </section>
        ))}
      </div>
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text(
          '各来源独立读取。服务调用“待完成”只表示已预留，不能当作成功；未归属本人的认证失败不计入个人登录事件。',
          'Each source is read independently. Pending service calls represent reservations, not success. Authentication failures without your user identity are excluded from personal login events.',
        )}
      </p>
    </div>
  );
}
