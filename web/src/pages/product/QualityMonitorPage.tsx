import { useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';
import type { ProductModule } from '../../product/catalog-types';
import type { ResourcePage } from '../../product/resource-types';
import ProductModulePage from '../../product/ProductModulePage';
import QualityAlertPanel, { QualityObservationSummary } from './QualityAlertPanel';
import type {
  QualityAlertView,
  QualityEvaluation,
  QualityNotice,
  QualityObservation,
} from './quality-alert-types';

export default function QualityMonitorPage({ module }: { module: ProductModule }) {
  const scope = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <Monitor key={scope} module={module} scope={scope} />;
}
function Monitor({ module, scope }: { module: ProductModule; scope: string }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const token = useAuthStore((state) => state.token);
  const canRead = useAuthStore((state) => !!state.user?.permissions.includes('quality:read'));
  const canWrite = useAuthStore((state) => !!state.user?.permissions.includes('quality:write'));
  const [tab, setTab] = useState<'monitor' | 'definitions' | 'notices'>('monitor');
  const [search, setSearch] = useState('');
  const [monitorOffset, setMonitorOffset] = useState(0);
  const [monitorId, setMonitorId] = useState('');
  const [view, setView] = useState<'observations' | 'alerts'>('observations');
  const [status, setStatus] = useState('');
  const [offset, setOffset] = useState(0);
  const [selectedAlert, setSelectedAlert] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const keys = useRef(new Map<string, string>());
  const cache = useQueryClient();
  const headers = { Authorization: `Bearer ${token}` };
  const monitors = useQuery({
    queryKey: ['quality-monitors', scope, search, monitorOffset],
    queryFn: ({ signal }) =>
      client
        .get<ResourcePage>('/product/resources/quality_monitor', {
          headers,
          signal,
          params: { q: search || undefined, limit: 20, offset: monitorOffset },
        })
        .then((response) => response.data),
    enabled: !!token && canRead && tab === 'monitor',
  });
  const selected = monitors.data?.data.find((record) => record.id === monitorId);
  const observations = useQuery({
    queryKey: ['quality-observations', scope, monitorId, offset],
    queryFn: ({ signal }) =>
      client
        .get<{ data: QualityObservation[]; total: number }>(
          `/product/quality-monitors/${encodeURIComponent(monitorId)}/observations`,
          { headers, signal, params: { limit: 10, offset } },
        )
        .then((response) => response.data),
    enabled: !!token && canRead && !!selected && tab === 'monitor' && view === 'observations',
    staleTime: 0,
  });
  const alerts = useQuery({
    queryKey: ['quality-alerts', scope, monitorId, status, offset],
    queryFn: ({ signal }) =>
      client
        .get<{ data: QualityAlertView[]; total: number }>(
          `/product/quality-monitors/${encodeURIComponent(monitorId)}/alerts`,
          { headers, signal, params: { status: status || undefined, limit: 10, offset } },
        )
        .then((response) => response.data),
    enabled: !!token && canRead && !!selected && tab === 'monitor' && view === 'alerts',
    staleTime: 0,
  });
  const detail =
    !alerts.isFetching && !alerts.isError
      ? alerts.data?.data.find((record) => record.alert.id === selectedAlert)
      : undefined;
  const refresh = async () => {
    await Promise.all(
      [
        'quality-observations',
        'quality-alerts',
        'quality-alert-events',
        'quality-notices',
        'quality-monitors',
      ].map((name) => cache.invalidateQueries({ queryKey: [name, scope] })),
    );
  };
  const evaluate = async () => {
    if (!selected || !canWrite) return;
    const session = captureSession();
    const signature = `${selected.id}:${selected.version}`;
    const key = keys.current.get(signature) ?? crypto.randomUUID();
    keys.current.set(signature, key);
    setBusy(true);
    setError('');
    try {
      await client.post<QualityEvaluation>(
        `/product/quality-monitors/${encodeURIComponent(selected.id)}/evaluate`,
        { expected_version: selected.version, idempotency_key: key },
        { headers },
      );
      if (!isCurrentSession(session)) return;
      keys.current.delete(signature);
      await refresh();
    } catch (error) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(error, text('读取或提交失败', 'Failed to load or submit')));
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  const page = view === 'alerts' ? alerts : observations;
  const change = (id: string) => {
    setMonitorId(id);
    setOffset(0);
    setSelectedAlert('');
    setError('');
  };
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">
          {text('质量监控与告警', 'Quality monitors and alerts')}
        </h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {text(
            '基于真实质量运行评估；缺少结果、失败或窗口不完整时不可评估。',
            'Evaluate actual quality runs; missing results, failures and incomplete windows are unavailable.',
          )}
        </p>
      </div>
      <div
        role="tablist"
        aria-label={text('质量监控工作区', 'Quality monitoring workspace')}
        className="flex flex-wrap gap-3 border-b border-[var(--color-border-default)]"
      >
        {(['monitor', 'definitions', 'notices'] as const).map((value) => (
          <button
            key={value}
            role="tab"
            aria-selected={tab === value}
            onClick={() => {
              setTab(value);
              setError('');
            }}
            className={`border-b-2 px-3 py-3 text-sm ${tab === value ? 'border-[var(--color-accent)] text-[var(--color-accent)]' : 'border-transparent'}`}
          >
            {value === 'monitor'
              ? text('实际观察与告警', 'Observed evaluations and alerts')
              : value === 'definitions'
                ? text('监控定义', 'Monitor definitions')
                : text('我的未读提醒', 'My unread notices')}
          </button>
        ))}
      </div>
      {tab === 'definitions' ? (
        <ProductModulePage module={module} />
      ) : tab === 'notices' ? (
        <QualityNotices />
      ) : !canRead ? (
        <ErrorBanner
          message={text('当前账号没有质量查看权限', 'Your account cannot read quality monitors')}
        />
      ) : (
        <>
          <div className="flex flex-col gap-3 rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-4 sm:flex-row sm:items-end">
            <label className="text-sm">
              {text('搜索监控', 'Search monitors')}
              <input
                value={search}
                maxLength={128}
                onChange={(event) => {
                  setSearch(event.target.value);
                  setMonitorOffset(0);
                  change('');
                }}
                className="mt-1 block rounded-md border border-[var(--color-border-default)] bg-transparent px-3 py-2"
              />
            </label>
            <label className="flex-1 text-sm">
              {text('当前页监控', 'Monitors on this page')}
              <select
                value={monitorId}
                onChange={(event) => change(event.target.value)}
                disabled={monitors.isFetching || monitors.isError}
                className="mt-1 block w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] px-3 py-2"
              >
                <option value="">{text('选择真实监控', 'Select a saved monitor')}</option>
                {!monitors.isError &&
                  monitors.data?.data.map((record) => (
                    <option key={record.id} value={record.id}>
                      {record.name} · {record.code} · v{record.version}
                    </option>
                  ))}
              </select>
            </label>
            <Button
              onClick={evaluate}
              disabled={busy || !selected || !canWrite || monitors.isFetching || monitors.isError}
            >
              {busy ? text('评估中…', 'Evaluating…') : text('评估并记录', 'Evaluate and record')}
            </Button>
            <Button variant="outline" onClick={refresh} disabled={busy}>
              {text('重新核验', 'Recheck')}
            </Button>
          </div>
          {monitors.isError && (
            <ErrorBanner
              message={apiErrorMessage(
                monitors.error,
                text('读取或提交失败', 'Failed to load or submit'),
              )}
            />
          )}
          {error && <ErrorBanner message={error} />}
          <Pagination
            offset={monitorOffset}
            step={20}
            total={monitors.isError ? undefined : monitors.data?.total}
            busy={monitors.isFetching}
            text={text}
            onChange={(value) => {
              setMonitorOffset(value);
              change('');
            }}
            label={text('监控定义', 'Monitor definitions')}
          />
          <p className="text-xs text-[var(--color-text-tertiary)]">
            {text(
              '手动评估会冻结规则版本、真实运行和 UTC 时间窗；当前页面不会假装已启用自动调度。',
              'Manual evaluation freezes rule versions, actual runs and the UTC window. Automatic scheduling is not enabled here.',
            )}
          </p>
          {selected && (
            <>
              <div className="flex flex-wrap items-center gap-3">
                {(['observations', 'alerts'] as const).map((value) => (
                  <Button
                    variant={view === value ? 'default' : 'outline'}
                    key={value}
                    onClick={() => {
                      setView(value);
                      setOffset(0);
                      setSelectedAlert('');
                    }}
                  >
                    {value === 'observations'
                      ? text('历史观察', 'Recorded observations')
                      : text('告警实例', 'Alert incidents')}
                  </Button>
                ))}
                {view === 'alerts' && (
                  <label className="text-sm">
                    {text('告警状态', 'Alert status')}
                    <select
                      value={status}
                      onChange={(event) => {
                        setStatus(event.target.value);
                        setOffset(0);
                        setSelectedAlert('');
                      }}
                      className="ml-2 rounded border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-2"
                    >
                      <option value="">{text('全部状态', 'All states')}</option>
                      <option value="open">{text('待处理', 'Open')}</option>
                      <option value="acknowledged">{text('已确认', 'Acknowledged')}</option>
                      <option value="resolved">{text('已恢复', 'Resolved')}</option>
                    </select>
                  </label>
                )}
              </div>
              {page.isFetching ? (
                <p>{text('重新核验当前数据授权…', 'Rechecking current data grants…')}</p>
              ) : page.isError ? (
                <ErrorBanner
                  message={apiErrorMessage(
                    page.error,
                    text('读取或提交失败', 'Failed to load or submit'),
                  )}
                />
              ) : view === 'observations' ? (
                <>
                  {!observations.data?.data.length && (
                    <p className="rounded-lg border border-dashed border-[var(--color-border-default)] p-6 text-sm">
                      {text(
                        '暂无持久化观察；先运行质量任务，再评估监控。',
                        'No recorded observation. Run a quality task, then evaluate this monitor.',
                      )}
                    </p>
                  )}
                  {observations.data?.data.map((observation) => (
                    <article
                      key={observation.id}
                      className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-4"
                    >
                      <p className="mb-3 text-sm text-[var(--color-text-tertiary)]">
                        {observation.created_at} · {observation.actor_id}
                      </p>
                      <QualityObservationSummary observation={observation} />
                    </article>
                  ))}
                </>
              ) : (
                <>
                  {!alerts.data?.data.length && (
                    <p>{text('当前筛选下暂无实际告警', 'No recorded alerts match this filter')}</p>
                  )}
                  {alerts.data?.data.map((record) => (
                    <button
                      key={record.alert.id}
                      onClick={() => setSelectedAlert(record.alert.id)}
                      className="block w-full rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-4 text-left hover:border-[var(--color-accent)]"
                    >
                      <span>
                        {record.alert.status === 'open'
                          ? text('待处理', 'Open')
                          : record.alert.status === 'acknowledged'
                            ? text('已确认', 'Acknowledged')
                            : text('已恢复', 'Resolved')}
                      </span>
                      <span className="ml-3 text-xs text-[var(--color-text-tertiary)]">
                        {record.alert.updated_at} · v{record.alert.version}
                      </span>
                    </button>
                  ))}
                </>
              )}
              <Pagination
                offset={offset}
                step={10}
                total={page.isError ? undefined : page.data?.total}
                busy={page.isFetching}
                text={text}
                onChange={(value) => {
                  setOffset(value);
                  setSelectedAlert('');
                }}
                label={view === 'alerts' ? text('告警', 'Alerts') : text('观察', 'Observations')}
              />
              {detail && (
                <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-4">
                  <QualityAlertPanel
                    key={`${scope}:${detail.alert.id}:${detail.alert.version}`}
                    record={detail}
                    onAcknowledge={async (payload) => {
                      const session = captureSession();
                      await client.post(
                        `/product/quality-alerts/${encodeURIComponent(detail.alert.id)}/acknowledge`,
                        payload,
                        { headers },
                      );
                      if (isCurrentSession(session)) await refresh();
                    }}
                  />
                </div>
              )}
            </>
          )}
        </>
      )}
    </div>
  );
}
function Pagination({
  offset,
  step,
  total,
  busy,
  onChange,
  text,
  label,
}: {
  offset: number;
  step: number;
  total?: number;
  busy: boolean;
  onChange: (value: number) => void;
  text: (zh: string, en: string) => string;
  label: string;
}) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <Button variant="outline" disabled={!offset || busy} onClick={() => onChange(offset - step)}>
        {text('上一页', 'Previous')}
      </Button>
      <span className="text-xs text-[var(--color-text-tertiary)]">
        {label} · {text('第', 'Page')} {Math.floor(offset / step) + 1} · {text('共', 'Total')}{' '}
        {total ?? '—'}
      </span>
      <Button
        variant="outline"
        disabled={busy || offset + step >= (total ?? 0)}
        onClick={() => onChange(offset + step)}
      >
        {text('下一页', 'Next')}
      </Button>
    </div>
  );
}
export function QualityNotices() {
  const scope = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <QualityNoticeInbox key={scope} scope={scope} />;
}
function QualityNoticeInbox({ scope }: { scope: string }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const token = useAuthStore((state) => state.token);
  const [unread, setUnread] = useState(true);
  const [offset, setOffset] = useState(0);
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');
  const cache = useQueryClient();
  const notices = useQuery({
    queryKey: ['quality-notices', scope, unread, offset],
    queryFn: ({ signal }) =>
      client
        .get<{ data: QualityNotice[]; total: number }>('/product/quality-notices', {
          signal,
          headers: { Authorization: `Bearer ${token}` },
          params: { unread_only: unread, limit: 10, offset },
        })
        .then((response) => response.data),
    enabled: !!token,
    staleTime: 0,
  });
  const read = async (id: string) => {
    const session = captureSession();
    setBusy(id);
    setError('');
    try {
      await client.post(
        `/product/quality-notices/${encodeURIComponent(id)}/read`,
        {},
        { headers: { Authorization: `Bearer ${token}` } },
      );
      if (isCurrentSession(session))
        await cache.invalidateQueries({ queryKey: ['quality-notices', scope] });
    } catch (error) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(error, text('读取或提交失败', 'Failed to load or submit')));
    } finally {
      if (isCurrentSession(session)) setBusy('');
    }
  };
  return (
    <section className="space-y-4">
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={unread}
          onChange={(event) => {
            setUnread(event.target.checked);
            setOffset(0);
          }}
        />
        {text('只显示本人未读提醒', 'Show only my unread notices')}
      </label>
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text(
          '提醒只含名称和链接；pending 表示通知发件箱尚未由消息投递驱动发送。读取提醒不授予数据访问权限。',
          'Notices contain only names and links. Pending means the delivery driver has not sent this outbox entry. Reading a notice grants no data access.',
        )}
      </p>
      {error && <ErrorBanner message={error} />}
      {notices.isFetching ? (
        <p>{text('读取本人提醒…', 'Loading my notices…')}</p>
      ) : notices.isError ? (
        <ErrorBanner
          message={apiErrorMessage(
            notices.error,
            text('读取或提交失败', 'Failed to load or submit'),
          )}
        />
      ) : (
        <>
          {!notices.data?.data.length && (
            <p>{text('暂无本人提醒', 'No notices for your account')}</p>
          )}
          {notices.data?.data.map((notice) => (
            <article
              key={notice.id}
              className="space-y-2 rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-4"
            >
              <p>{notice.subject}</p>
              <p className="text-xs text-[var(--color-text-tertiary)]">
                {notice.created_at} ·{' '}
                {notice.delivery_status === 'pending'
                  ? text('发件箱待投递', 'Outbox pending delivery')
                  : notice.delivery_status === 'unconfigured'
                    ? text('投递驱动未配置', 'Delivery driver unconfigured')
                    : notice.delivery_status === 'failed'
                      ? text('实际投递失败', 'Delivery failed')
                      : text('实际已投递', 'Delivered')}
              </p>
              <div className="flex gap-3">
                <a href="/quality/monitoring" className="text-sm text-[var(--color-accent)]">
                  {text(
                    '查看监控（需当前数据授权）',
                    'Open monitoring (current data grants required)',
                  )}
                </a>
                {!notice.read_at ? (
                  <Button variant="outline" disabled={!!busy} onClick={() => read(notice.id)}>
                    {busy === notice.id
                      ? text('记录中…', 'Recording…')
                      : text('标为已读', 'Mark as read')}
                  </Button>
                ) : (
                  <span>{text('已读', 'Read')}</span>
                )}
              </div>
            </article>
          ))}
        </>
      )}
      <Pagination
        offset={offset}
        step={10}
        total={notices.isError ? undefined : notices.data?.total}
        busy={notices.isFetching}
        text={text}
        onChange={setOffset}
        label={text('本人提醒', 'My notices')}
      />
    </section>
  );
}
