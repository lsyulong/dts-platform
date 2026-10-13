import { useState } from 'react';
import { useParams, Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, Square, Play, Clock, ExternalLink } from 'lucide-react';
import MetricCard from '../../components/MetricCard';
import { EmptyState, StatusBadge, FormInput, ErrorBanner } from '../../components/ui';
import { TaskDetailSkeleton } from '../../components/skeletons';
import { AiReportPanel } from '../../components/ai';
import {
  useTask,
  useUpdateTaskStatus,
  useScheduleStatus,
  useSetSchedule,
  useDeleteSchedule,
} from '../../hooks/useApi';
import { useTaskProgress, formatBytes, formatRows } from '../../hooks/useTaskProgress';
import { apiErrorMessage } from '../../lib/apiError';
import { useAuthStore } from '../../stores/auth';

export default function TaskDetailPage() {
  const { t } = useTranslation();
  const { id } = useParams<{ id: string }>();
  const { data: task, isLoading, error: taskError, refetch } = useTask(id);
  const updateStatus = useUpdateTaskStatus();
  const user = useAuthStore((state) => state.user);
  const canExecute = user?.permissions.includes('task:execute') ?? false;
  const cancelling = updateStatus.isSuccess && updateStatus.variables?.status === 'cancelled';
  const progress = useTaskProgress(id);
  const {
    data: schedule,
    isLoading: scheduleLoading,
    error: scheduleError,
  } = useScheduleStatus(id);

  const setSchedule = useSetSchedule();
  const deleteSchedule = useDeleteSchedule();
  const [cronInput, setCronInput] = useState('');
  const [showCronInput, setShowCronInput] = useState(false);

  const statusLabel: Record<string, string> = {
    draft: t('tasks.status.draft'),
    running: t('tasks.status.running'),
    paused: t('tasks.status.paused'),
    completed: t('tasks.status.completed'),
    failed: t('tasks.status.failed'),
    cancelled: t('tasks.status.cancelled'),
  };

  if (isLoading) {
    return <TaskDetailSkeleton />;
  }

  if (taskError || !task) {
    return (
      <div className="space-y-4">
        <ErrorBanner message={t('tasks.detail.loadError')} onRetry={() => void refetch()} />
        <Link to="/tasks" className="text-sm text-[var(--color-accent)]">
          {t('tasks.title')}
        </Link>
      </div>
    );
  }

  const pct = progress.latest
    ? progress.latest.percent > 0
      ? progress.latest.percent
      : task.recent_runs.length > 0 && task.recent_runs[0].total_rows > 0
        ? (task.recent_runs[0].rows_synced / task.recent_runs[0].total_rows) * 100
        : 0
    : task.recent_runs.length > 0 && task.recent_runs[0].total_rows > 0
      ? (task.recent_runs[0].rows_synced / task.recent_runs[0].total_rows) * 100
      : 0;

  const isLiveRunning = progress.connected && progress.latest?.status === 'Running';
  const liveRows = progress.latest?.rows_synced ?? task.recent_runs[0]?.rows_synced ?? 0;
  const metricsAvailable = progress.latest?.metrics_available !== false;
  const scheduleStatusLabel = (status: string) =>
    status === 'triggered' ? t('ops.dispatched') : status;
  const scheduleState = schedule?.active ? 'active' : (schedule?.state ?? 'inactive');

  return (
    <div className="space-y-6">
      {progress.accessDenied && (
        <div role="alert">
          <ErrorBanner message={t('tasks.detail.logsAccessDenied')} />
        </div>
      )}
      {updateStatus.error && (
        <ErrorBanner
          message={`${t('tasks.updateStatusError')}: ${apiErrorMessage(updateStatus.error, t('errors.tryAgainLater'))}`}
        />
      )}
      <div className="flex items-center gap-4">
        <Link
          to="/tasks"
          className="text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)]"
        >
          <ArrowLeft size={20} />
        </Link>
        <div className="flex-1">
          <h1 className="text-xl font-bold text-[var(--color-text-primary)]">{task.name}</h1>
          <div className="mt-1 flex items-center gap-3">
            <span className="text-xs text-[var(--color-text-tertiary)]">
              ID: {task.id.slice(0, 8)}...
            </span>
            <span className="text-xs text-[var(--color-text-disabled)]">
              {task.source_type} → {task.target_type}
            </span>
            {task.cron_expr && (
              <span className="rounded-full bg-[var(--color-bg-tertiary)] px-2 py-0.5 text-xs text-[var(--color-text-secondary)]">
                {task.cron_expr}
              </span>
            )}
          </div>
        </div>
        {canExecute && (
          <div className="flex gap-2">
            {task.status === 'running' ? (
              <button
                onClick={() => updateStatus.mutate({ id: task.id, status: 'cancelled' })}
                disabled={updateStatus.isPending || cancelling}
                className="flex items-center gap-1.5 rounded-lg border border-amber-500/30 px-3 py-2 text-sm text-[var(--color-warning)] hover:bg-amber-500/10 disabled:opacity-50"
              >
                <Square size={16} />{' '}
                {t(cancelling ? 'tasks.detail.cancelling' : 'tasks.detail.cancelRun')}
              </button>
            ) : (
              <button
                onClick={() => updateStatus.mutate({ id: task.id, status: 'running' })}
                disabled={updateStatus.isPending}
                className="flex items-center gap-1.5 rounded-lg bg-emerald-500 px-3 py-2 text-sm font-medium text-white hover:bg-emerald-600 disabled:opacity-50"
              >
                <Play size={16} />{' '}
                {task.status === 'draft' ? t('tasks.detail.start') : t('tasks.detail.rerun')}
              </button>
            )}
          </div>
        )}
      </div>

      {(task.status === 'running' || isLiveRunning) && (
        <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5">
          <div className="flex items-center justify-between">
            <h3 className="flex items-center gap-2 text-sm font-medium text-[var(--color-text-secondary)]">
              {t('tasks.detail.realTimeProgress')}
              {isLiveRunning && (
                <span className="inline-block h-2 w-2 rounded-full bg-emerald-400 animate-pulse" />
              )}
            </h3>
            {metricsAvailable && (
              <span className="text-sm text-[var(--color-text-secondary)]">{pct.toFixed(1)}%</span>
            )}
          </div>
          {metricsAvailable ? (
            <div className="mt-3 h-2 overflow-hidden rounded-full bg-[var(--color-bg-tertiary)]">
              <div
                className="h-full rounded-full bg-gradient-to-r from-[var(--color-accent)] to-emerald-400 transition-all duration-500"
                style={{ width: `${Math.min(pct, 100)}%` }}
              />
            </div>
          ) : (
            <p role="status" className="mt-3 text-sm text-[var(--color-text-tertiary)]">
              {t('tasks.detail.metricsUnavailable')}
            </p>
          )}
          {progress.latest && metricsAvailable && (
            <div className="mt-3 grid grid-cols-2 gap-3 text-xs text-[var(--color-text-tertiary)] sm:grid-cols-3">
              <div>
                <span className="text-[var(--color-text-disabled)]">
                  {t('tasks.detail.rowsSynced', { count: '' })}
                </span>
                <p className="text-[var(--color-text-secondary)]">{formatRows(liveRows)}</p>
              </div>
              <div>
                <span className="text-[var(--color-text-disabled)]">{t('tasks.detail.bytes')}</span>
                <p className="text-[var(--color-text-secondary)]">
                  {formatBytes(progress.latest.bytes_synced)}
                </p>
              </div>
              {progress.latest.current_table && (
                <div>
                  <span className="text-[var(--color-text-disabled)]">
                    {t('tasks.detail.table')}
                  </span>
                  <p className="text-[var(--color-text-secondary)] truncate">
                    {progress.latest.current_table}
                  </p>
                </div>
              )}
            </div>
          )}
        </div>
      )}

      <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5 backdrop-blur-sm">
        <div className="flex items-center justify-between">
          <h3 className="flex items-center gap-2 text-sm font-medium text-[var(--color-text-secondary)]">
            <Clock size={16} />
            {t('ops.instances')}
          </h3>
          <a
            href="http://localhost:9090"
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-1 text-xs text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)]"
          >
            jiascheduler <ExternalLink size={10} />
          </a>
        </div>

        {scheduleLoading ? (
          <p className="mt-3 text-xs text-[var(--color-text-disabled)]">{t('common.loading')}</p>
        ) : scheduleError ? (
          <div className="mt-3">
            <ErrorBanner message={t('tasks.detail.scheduleError')} />
          </div>
        ) : schedule?.enabled === false ? (
          <p className="mt-3 text-xs text-[var(--color-text-tertiary)]">
            {t('tasks.detail.schedulerUnavailable')}
          </p>
        ) : schedule?.has_timer ? (
          <div className="mt-3 space-y-2">
            <div className="flex items-center gap-3 text-sm">
              <span className="text-[var(--color-text-secondary)]">Cron (UTC):</span>
              <code className="rounded bg-[var(--color-bg-tertiary)] px-2 py-0.5 text-xs text-[var(--color-success)]">
                {schedule.cron_expr}
              </code>
              <span
                className={`inline-block rounded-full px-2 py-0.5 text-xs ${schedule?.active ? 'bg-emerald-500/10 text-[var(--color-success)]' : 'bg-amber-500/10 text-[var(--color-warning)]'}`}
              >
                {t(`ops.scheduleStates.${scheduleState}`)}
              </span>
            </div>
            {schedule.schedule_history.length > 0 && (
              <div className="space-y-1">
                <p className="text-xs text-[var(--color-text-tertiary)]">
                  {t('ops.dispatchHistory')} ({schedule.schedule_history.length})
                </p>
                {schedule.schedule_history.slice(0, 3).map((h) => (
                  <div
                    key={h.id}
                    className="flex items-center justify-between text-xs text-[var(--color-text-tertiary)]"
                  >
                    <span>{h.created_time ?? '--'}</span>
                    <span className="text-[var(--color-accent)]">
                      {scheduleStatusLabel(h.status)}
                    </span>
                  </div>
                ))}
              </div>
            )}
            {canExecute && (
              <button
                onClick={() => deleteSchedule.mutate(id ?? '')}
                disabled={deleteSchedule.isPending}
                className="rounded-lg border border-[var(--color-border-hover)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] disabled:opacity-50"
              >
                {t('common.cancel')} {t('ops.instances')}
              </button>
            )}
          </div>
        ) : showCronInput && canExecute ? (
          <div className="mt-3 space-y-3">
            <FormInput
              label={t('ops.cronUtc')}
              placeholder="0 */6 * * *"
              value={cronInput}
              onChange={(v) => setCronInput(v)}
            />
            <div className="flex gap-2">
              <button
                onClick={() => {
                  if (cronInput.trim()) {
                    setSchedule.mutate(
                      { taskId: id ?? '', cronExpr: cronInput.trim() },
                      { onSuccess: () => setShowCronInput(false) },
                    );
                  }
                }}
                disabled={!cronInput.trim() || setSchedule.isPending}
                className="rounded-lg bg-[var(--color-action)] px-3 py-1.5 text-xs font-medium text-white hover:bg-[var(--color-action-hover)] disabled:opacity-50"
              >
                {setSchedule.isPending ? t('common.loading') : t('common.confirm')}
              </button>
              <button
                onClick={() => setShowCronInput(false)}
                className="rounded-lg border border-[var(--color-border-hover)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)]"
              >
                {t('common.cancel')}
              </button>
            </div>
          </div>
        ) : canExecute ? (
          <div className="mt-3">
            <button
              onClick={() => setShowCronInput(true)}
              className="rounded-lg border border-[var(--color-border-hover)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)]"
            >
              <Clock size={12} className="inline mr-1" />
              {t('ops.scheduled')}
            </button>
          </div>
        ) : (
          <p className="mt-3 text-xs text-[var(--color-text-tertiary)]">{t('ops.notScheduled')}</p>
        )}
        {(setSchedule.error || deleteSchedule.error) && (
          <div className="mt-3">
            <ErrorBanner
              message={`${t('tasks.detail.scheduleUpdateError')}: ${apiErrorMessage(setSchedule.error || deleteSchedule.error, t('errors.tryAgainLater'))}`}
            />
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        <MetricCard
          title={t('tasks.detail.source')}
          value={task.source_name}
          subtitle={task.source_type}
        />
        <MetricCard
          title={t('tasks.detail.target')}
          value={task.target_name}
          subtitle={task.target_type}
        />
        <MetricCard
          title={t('tasks.detail.status')}
          value={statusLabel[task.status] || task.status}
          subtitle={`${t('tasks.detail.createdAt')} ${task.created_at}`}
        />
      </div>

      <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5 backdrop-blur-sm">
        <h3 className="mb-4 text-sm font-medium text-[var(--color-text-secondary)]">
          {t('tasks.detail.executionHistory')}
        </h3>
        {task.recent_runs.length === 0 ? (
          <EmptyState title={t('tasks.detail.noExecutionRecords')} />
        ) : (
          <div className="space-y-2">
            {task.recent_runs.map((run) => (
              <div
                key={run.id}
                className="flex items-center justify-between rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-bg-secondary)] p-3"
              >
                <div>
                  <div className="flex items-center gap-2">
                    <p className="text-xs font-mono text-[var(--color-text-secondary)]">
                      #{run.id.slice(0, 8)}
                    </p>
                    <span className="rounded bg-[var(--color-accent-subtle)] px-1.5 py-0.5 text-[10px] text-[var(--color-accent)]">
                      Engine
                    </span>
                  </div>
                  <p className="text-xs text-[var(--color-text-tertiary)]">{run.start_time}</p>
                </div>
                <div className="text-right">
                  <p className="text-xs text-[var(--color-text-secondary)]">
                    {t('tasks.detail.rowsSynced', {
                      count: (run.rows_synced ?? 0).toLocaleString(),
                    })}
                  </p>
                  {run.error_message && (
                    <p className="mt-0.5 max-w-xl whitespace-pre-wrap break-words text-xs text-[var(--color-error)]">
                      {run.error_message}
                    </p>
                  )}
                </div>
                <StatusBadge status={run.status} />
              </div>
            ))}
          </div>
        )}
        {(schedule?.schedule_history.length ?? 0) > 0 && (
          <div className="mt-4 space-y-2 border-t border-[var(--color-border-subtle)] pt-4">
            <h3 className="text-sm font-medium text-[var(--color-text-secondary)]">
              {t('ops.dispatchHistory')}
            </h3>
            <p className="text-xs text-[var(--color-text-tertiary)]">
              {t('ops.dispatchHistoryHint')}
            </p>
            {schedule?.schedule_history.map((h) => (
              <div
                key={`sch-${h.id}`}
                className="flex items-center justify-between rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-bg-secondary)] p-3"
              >
                <div>
                  <div className="flex items-center gap-2">
                    <p className="text-xs font-mono text-[var(--color-text-tertiary)]">
                      sch#{h.id}
                    </p>
                    <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-[var(--color-warning)]">
                      Scheduler
                    </span>
                  </div>
                  <p className="text-xs text-[var(--color-text-tertiary)]">
                    {h.created_time ?? '--'}
                  </p>
                </div>
                <div className="text-right">
                  <p className="text-xs text-[var(--color-text-secondary)]">{h.name}</p>
                </div>
                <span className="inline-block rounded-full bg-[var(--color-accent-subtle)] px-2 py-0.5 text-xs font-medium text-[var(--color-accent)]">
                  {scheduleStatusLabel(h.status)}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {id && (
        <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5 backdrop-blur-sm">
          <h3 className="mb-4 text-sm font-medium text-[var(--color-text-secondary)]">
            {t('ai.reportTitle')}
          </h3>
          <AiReportPanel taskId={id} />
        </div>
      )}
    </div>
  );
}
