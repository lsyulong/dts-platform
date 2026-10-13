import { useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import {
  qualityReasons,
  type QualityAlertView,
  type QualityAckRequest,
  type QualityEvent,
  type QualityObservation,
} from './quality-alert-types';

export function QualityObservationSummary({ observation }: { observation: QualityObservation }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  if (observation.result_access !== 'allowed')
    return (
      <p>
        {text(
          '当前数据授权不可读取观察计数',
          'Current data grants do not allow reading these counts',
        )}
      </p>
    );
  const summary = observation.summary;
  const pair = qualityReasons[observation.reason];
  const label = pair ? text(...pair) : observation.reason;
  const ratio =
    summary.failure_ratio == null
      ? text('暂无可用比例', 'Ratio unavailable')
      : `${(summary.failure_ratio * 100).toFixed(2)}%`;
  return (
    <div className="space-y-3">
      <p
        className={`rounded-lg border px-3 py-2 text-sm ${observation.state === 'threshold_exceeded' ? 'border-red-300 text-red-600' : 'border-[var(--color-border-default)] text-[var(--color-text-secondary)]'}`}
      >
        {label}
      </p>
      <dl className="grid grid-cols-2 gap-3 text-sm sm:grid-cols-4">
        {[
          [text('实际校验次数', 'Observed row-rule checks'), summary.checked_rows],
          [text('实际失败次数', 'Observed failures'), summary.failed_rows],
          [text('失败比例', 'Failure ratio'), ratio],
          [
            text('成功 / 失败运行', 'Succeeded / failed runs'),
            `${summary.completed_runs} / ${summary.failed_runs}`,
          ],
        ].map(([label, value]) => (
          <div key={String(label)}>
            <dt className="text-[var(--color-text-tertiary)]">{label}</dt>
            <dd className="mt-1 break-words font-medium">{value ?? '—'}</dd>
          </div>
        ))}
      </dl>
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text(
          '统计单位为“数据行 × 规则检查”；不同规则检查同一行会分别计数。',
          'Counts represent row-rule checks; separate rules count the same row separately.',
        )}
      </p>
      {Number(summary.sampled_checks) > 0 && (
        <p className="text-sm text-amber-600">
          {text(
            '包含采样检查，结果不能代表全表通过。',
            'Includes sampled checks; this does not establish a full-table pass.',
          )}
        </p>
      )}
      {!summary.window_complete && (
        <p className="text-sm text-amber-600">
          {text(
            '窗口不完整，不会自动恢复现有告警。',
            'Incomplete windows never automatically resolve an existing alert.',
          )}
        </p>
      )}
      <p className="break-words text-xs text-[var(--color-text-tertiary)]">
        {text('实际 UTC 窗口', 'Observed UTC window')}: {observation.period_start} →{' '}
        {observation.period_end}
      </p>
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text('监控版本', 'Monitor version')}: v{observation.monitor_version} ·{' '}
        {text('真实运行数', 'Actual run count')}: {observation.run_ids.length}
      </p>
    </div>
  );
}

export default function QualityAlertPanel({
  record,
  onAcknowledge,
  disabled = false,
}: {
  record: QualityAlertView;
  onAcknowledge: (payload: QualityAckRequest) => Promise<void>;
  disabled?: boolean;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const token = useAuthStore((state) => state.token);
  const canRead = useAuthStore((state) => !!state.user?.permissions.includes('quality:read'));
  const canWrite = useAuthStore((state) => !!state.user?.permissions.includes('quality:write'));
  const scope = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  const [reason, setReason] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const key = useRef<{ signature: string; id: string } | undefined>(undefined);
  const [offset, setOffset] = useState(0);
  const alert = record.alert;
  const events = useQuery({
    queryKey: ['quality-alert-events', scope, alert.id, offset],
    queryFn: ({ signal }) =>
      client
        .get<{ data: QualityEvent[]; total: number }>(
          `/product/quality-alerts/${encodeURIComponent(alert.id)}/events`,
          { signal, params: { limit: 10, offset }, headers: { Authorization: `Bearer ${token}` } },
        )
        .then((response) => response.data),
    enabled: !!token && canRead,
    staleTime: 0,
  });
  const acknowledge = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (disabled || !canWrite || alert.status !== 'open') return;
    const signature = JSON.stringify([scope, alert.id, alert.version, reason.trim()]);
    if (key.current?.signature !== signature) key.current = { signature, id: crypto.randomUUID() };
    setBusy(true);
    setError('');
    try {
      await onAcknowledge({
        expected_version: alert.version,
        idempotency_key: key.current.id,
        reason: reason.trim(),
      });
      key.current = undefined;
      setReason('');
    } catch (error) {
      setError(apiErrorMessage(error, text('读取或提交失败', 'Failed to load or submit')));
    } finally {
      setBusy(false);
    }
  };
  if (!canRead)
    return (
      <ErrorBanner
        message={text('当前账号没有质量查看权限', 'Your account cannot read quality results')}
      />
    );
  return (
    <section className="space-y-5">
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="font-semibold">{text('告警详情', 'Alert details')}</h3>
        <span className="rounded bg-[var(--color-bg-page)] px-2 py-1 text-sm">
          {alert.status === 'open'
            ? text('待处理', 'Open')
            : alert.status === 'acknowledged'
              ? text('已确认', 'Acknowledged')
              : text('已恢复', 'Resolved')}
        </span>
        <span className="text-xs text-[var(--color-text-tertiary)]">v{alert.version}</span>
      </div>
      <QualityObservationSummary observation={record.observation} />
      {alert.status !== 'resolved' &&
        alert.monitor_version !== record.observation.monitor_version && (
          <p className="text-sm text-amber-600">
            {text(
              '监控策略版本已变更，当前观察不能以新阈值关闭旧告警。',
              'The monitor policy changed; the new threshold cannot resolve this earlier incident.',
            )}
          </p>
        )}
      {alert.acknowledged_by && (
        <p className="text-sm">
          {text('确认人', 'Acknowledged by')}: {alert.acknowledged_by} · {alert.acknowledged_at}
        </p>
      )}
      {alert.resolved_at && (
        <p className="text-sm">
          {text('实际恢复时间', 'Observed resolution time')}: {alert.resolved_at}
        </p>
      )}
      {alert.status === 'open' && canWrite && (
        <form
          onSubmit={acknowledge}
          className="space-y-3 border-t border-[var(--color-border-default)] pt-4"
        >
          <label className="block text-sm">
            {text('处理说明', 'Handling reason')}
            <textarea
              value={reason}
              onChange={(event) => setReason(event.target.value)}
              required
              maxLength={2000}
              disabled={busy || disabled}
              className="mt-2 w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] p-3 focus:border-[var(--color-accent)]"
            />
          </label>
          <p className="text-xs text-[var(--color-text-tertiary)]">
            {text(
              '确认表示已接手处理；后续完整质量评估恢复后才关闭告警。',
              'Acknowledgment records ownership; only a later complete quality evaluation can resolve the alert.',
            )}
          </p>
          {error && <ErrorBanner message={error} />}
          <Button type="submit" disabled={busy || disabled || !reason.trim()}>
            {busy ? text('提交中…', 'Submitting…') : text('确认处理', 'Acknowledge')}
          </Button>
        </form>
      )}
      <div className="space-y-2">
        <h4 className="text-sm font-semibold">{text('实际状态事件', 'Recorded state events')}</h4>
        {events.isFetching ? (
          <p>{text('重新核验事件访问权限…', 'Rechecking event access…')}</p>
        ) : events.isError ? (
          <ErrorBanner
            message={apiErrorMessage(
              events.error,
              text('读取或提交失败', 'Failed to load or submit'),
            )}
          />
        ) : (
          <>
            {!events.data?.data.length && (
              <p className="text-sm text-[var(--color-text-tertiary)]">
                {text('暂无事件', 'No recorded events')}
              </p>
            )}
            {events.data?.data.map((event) => (
              <div
                key={event.id}
                className="rounded-lg border border-[var(--color-border-default)] p-3 text-sm"
              >
                <p>
                  {event.action} · {event.before_status ?? '—'} → {event.after_status}
                </p>
                <p className="break-words">
                  {qualityReasons[event.reason]
                    ? text(...qualityReasons[event.reason])
                    : event.reason}
                </p>
                <p className="text-xs text-[var(--color-text-tertiary)]">
                  {event.actor_id} · {event.created_at}
                </p>
              </div>
            ))}
          </>
        )}
        <div className="flex items-center gap-3">
          <Button
            variant="outline"
            disabled={!offset || events.isFetching}
            onClick={() => setOffset(offset - 10)}
          >
            {text('上一页', 'Previous')}
          </Button>
          <span className="text-xs">
            {text('事件总数', 'Total events')}: {events.isError ? '—' : (events.data?.total ?? '—')}
          </span>
          <Button
            variant="outline"
            disabled={events.isFetching || offset + 10 >= (events.data?.total ?? 0)}
            onClick={() => setOffset(offset + 10)}
          >
            {text('下一页', 'Next')}
          </Button>
        </div>
      </div>
    </section>
  );
}
