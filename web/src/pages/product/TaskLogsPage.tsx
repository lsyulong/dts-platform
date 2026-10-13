import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { FileText, RefreshCw } from 'lucide-react';
import client from '../../api/client';
import LogConsole from '../../components/LogConsole';
import { Button, EmptyState, ErrorBanner, LoadingSpinner, StatusBadge } from '../../components/ui';
import { useTask, useTasks } from '../../hooks/useApi';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';

interface RunLog {
  id: string;
  sequence: number;
  timestamp: string;
  level: string;
  component: string;
  message: string;
}

interface RunLogsPage {
  data: RunLog[];
  total: number;
}

const pageSize = 200;
const selectClass =
  'w-full rounded-md border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm text-[var(--color-text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--color-accent)] disabled:opacity-50';

function isRunLog(value: unknown): value is RunLog {
  if (typeof value !== 'object' || value === null) return false;
  return (
    'id' in value &&
    typeof value.id === 'string' &&
    'sequence' in value &&
    typeof value.sequence === 'number' &&
    Number.isSafeInteger(value.sequence) &&
    'timestamp' in value &&
    typeof value.timestamp === 'string' &&
    'level' in value &&
    typeof value.level === 'string' &&
    'component' in value &&
    typeof value.component === 'string' &&
    'message' in value &&
    typeof value.message === 'string'
  );
}

function parseLogsPage(value: unknown, invalidMessage: string): RunLogsPage {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('data' in value) ||
    !Array.isArray(value.data) ||
    !value.data.every(isRunLog) ||
    !('total' in value) ||
    typeof value.total !== 'number' ||
    !Number.isSafeInteger(value.total) ||
    value.total < 0
  )
    throw new Error(invalidMessage);
  return { data: value.data, total: value.total };
}

export default function TaskLogsPage() {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const tasks = useTasks();
  const [taskId, setTaskId] = useState('');
  const [runId, setRunId] = useState('');
  const [offset, setOffset] = useState(0);
  const [level, setLevel] = useState('ALL');
  const task = useTask(taskId || undefined);
  const selectedRun = task.data?.recent_runs.find((run) => run.id === runId);
  const session = captureSession();
  const logs = useQuery({
    queryKey: ['task-run-logs', session.tenantId, session.userId, taskId, runId, offset],
    enabled: !!session.token && !!taskId && !!selectedRun && task.data?.id === taskId,
    retry: false,
    queryFn: async ({ signal }) => {
      if (!taskId || !runId || !session.token)
        throw new Error(text('请先选择运行实例', 'Select a run first'));
      const response = await client.get<unknown>(
        `/tasks/${encodeURIComponent(taskId)}/runs/${encodeURIComponent(runId)}/logs`,
        {
          params: { offset, limit: pageSize },
          signal,
          headers: { Authorization: `Bearer ${session.token}` },
        },
      );
      if (!isCurrentSession(session))
        throw new Error(text('登录会话已变更', 'The account session has changed'));
      return parseLogsPage(
        response.data,
        text('服务器返回的日志格式不正确', 'The server returned an invalid log response'),
      );
    },
  });
  const pageLogs = logs.data?.data ?? [];
  const filtered =
    level === 'ALL' ? pageLogs : pageLogs.filter((entry) => entry.level.toUpperCase() === level);
  const total = logs.data?.total ?? 0;
  const selectTask = (value: string) => {
    setTaskId(value);
    setRunId('');
    setOffset(0);
    setLevel('ALL');
  };
  const refresh = async () => {
    await tasks.refetch();
    if (!isCurrentSession(session)) return;
    if (taskId) await task.refetch();
    if (!isCurrentSession(session)) return;
    if (selectedRun) await logs.refetch();
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="flex items-center gap-2 text-xl font-semibold text-[var(--color-text-primary)]">
            <FileText size={22} />
            {text('任务日志', 'Task logs')}
          </h1>
          <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
            {text(
              '按任务与运行实例查看实际执行日志',
              'Inspect actual execution logs by task and run instance',
            )}
          </p>
        </div>
        <Button
          variant="outline"
          onClick={() => void refresh()}
          disabled={tasks.isFetching || task.isFetching || logs.isFetching}
        >
          <RefreshCw size={16} />
          {text('刷新', 'Refresh')}
        </Button>
      </div>
      {tasks.error && (
        <div role="alert">
          <ErrorBanner
            message={apiErrorMessage(tasks.error, text('任务列表加载失败', 'Failed to load tasks'))}
            onRetry={() => void tasks.refetch()}
          />
        </div>
      )}
      <section className="data-panel p-4 sm:p-5" aria-label={text('日志筛选', 'Log filters')}>
        <div className="grid gap-4 md:grid-cols-3">
          <div>
            <label
              htmlFor="log-task"
              className="mb-1.5 block text-sm font-medium text-[var(--color-text-secondary)]"
            >
              {text('选择任务', 'Select task')}
            </label>
            <select
              id="log-task"
              value={taskId}
              onChange={(event) => selectTask(event.target.value)}
              disabled={tasks.isLoading}
              className={selectClass}
            >
              <option value="">{text('请选择任务', 'Choose a task')}</option>
              {(tasks.data ?? []).map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label
              htmlFor="log-run"
              className="mb-1.5 block text-sm font-medium text-[var(--color-text-secondary)]"
            >
              {text('选择运行实例', 'Select run')}
            </label>
            <select
              id="log-run"
              value={runId}
              disabled={!taskId || task.isLoading || !task.data?.recent_runs.length}
              onChange={(event) => {
                setRunId(event.target.value);
                setOffset(0);
              }}
              className={selectClass}
            >
              <option value="">{text('请选择运行实例', 'Choose a run')}</option>
              {(task.data?.recent_runs ?? []).map((run) => (
                <option key={run.id} value={run.id}>
                  {run.start_time} · {run.id}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label
              htmlFor="log-level"
              className="mb-1.5 block text-sm font-medium text-[var(--color-text-secondary)]"
            >
              {text('当前页日志级别', 'Log level on this page')}
            </label>
            <select
              id="log-level"
              value={level}
              onChange={(event) => setLevel(event.target.value)}
              className={selectClass}
            >
              <option value="ALL">{text('全部级别', 'All levels')}</option>
              {['TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR', 'FATAL'].map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </div>
        </div>
        <p className="mt-3 text-xs text-[var(--color-text-tertiary)]">
          {text(
            '日志级别仅筛选当前加载页；每页最多 200 条。运行列表来自任务近期实例。',
            'Level filtering applies only to the loaded page, with up to 200 entries per page. The run selector shows recent task instances.',
          )}
        </p>
      </section>
      {task.error && (
        <div role="alert">
          <ErrorBanner
            message={apiErrorMessage(
              task.error,
              text('任务详情加载失败', 'Failed to load task details'),
            )}
            onRetry={() => void task.refetch()}
          />
        </div>
      )}
      {selectedRun && (
        <section
          className="data-panel flex flex-wrap items-center gap-4 p-4"
          aria-label={text('运行信息', 'Run information')}
        >
          <StatusBadge status={selectedRun.status} />
          <span className="break-all text-sm text-[var(--color-text-secondary)]">
            {text('运行 ID', 'Run ID')}: {selectedRun.id}
          </span>
          <span className="text-sm text-[var(--color-text-tertiary)]">
            {selectedRun.start_time}
          </span>
        </section>
      )}
      {tasks.isLoading || (taskId && task.isLoading) ? (
        <LoadingSpinner />
      ) : tasks.error && !taskId ? null : !taskId ? (
        <EmptyState
          icon={FileText}
          title={text(
            tasks.data?.length ? '选择任务以查看运行日志' : '当前租户没有同步任务',
            tasks.data?.length
              ? 'Select a task to inspect logs'
              : 'This tenant has no integration tasks',
          )}
        />
      ) : !task.error && !task.data?.recent_runs.length ? (
        <EmptyState
          icon={FileText}
          title={text('该任务尚无运行实例', 'This task has no recorded runs')}
          description={text(
            '任务执行后可在这里选择实际运行并查看日志。',
            'Recorded executions will appear here after the task runs.',
          )}
        />
      ) : !runId ? (
        <EmptyState icon={FileText} title={text('请选择运行实例', 'Select a run instance')} />
      ) : logs.error ? (
        <div role="alert">
          <ErrorBanner
            message={apiErrorMessage(
              logs.error,
              text('运行日志加载失败', 'Failed to load run logs'),
            )}
            onRetry={() => void logs.refetch()}
          />
        </div>
      ) : logs.isLoading ? (
        <LoadingSpinner />
      ) : logs.data ? (
        <section className="space-y-4" aria-label={text('运行日志内容', 'Run log entries')}>
          <div role="status" className="text-sm text-[var(--color-text-tertiary)]">
            {text('总记录', 'Total entries')}: {total} · {text('当前页', 'Current page')}:{' '}
            {Math.floor(offset / pageSize) + 1} · {text('筛选后显示', 'Shown after filtering')}:{' '}
            {filtered.length}
          </div>
          {filtered.length ? (
            <LogConsole
              hideLevelFilters
              title={text('实际运行日志', 'Recorded run logs')}
              logs={filtered}
              maxHeight="560px"
            />
          ) : (
            <EmptyState
              icon={FileText}
              title={text(
                pageLogs.length ? '当前页没有此级别日志' : '此运行尚未记录日志',
                pageLogs.length
                  ? 'No entries match this level on the current page'
                  : 'No logs have been recorded for this run',
              )}
            />
          )}
          <div className="flex flex-wrap items-center justify-between gap-3">
            <span className="text-xs text-[var(--color-text-tertiary)]">
              {pageLogs.length
                ? `${offset + 1}–${Math.min(offset + pageLogs.length, total)} / ${total}`
                : `0 / ${total}`}
            </span>
            <div className="flex gap-2">
              <Button
                variant="outline"
                onClick={() => setOffset(Math.max(0, offset - pageSize))}
                disabled={!offset || logs.isFetching}
              >
                {text('上一页', 'Previous page')}
              </Button>
              <Button
                variant="outline"
                onClick={() => setOffset(offset + pageSize)}
                disabled={offset + pageSize >= total || logs.isFetching}
              >
                {text('下一页', 'Next page')}
              </Button>
            </div>
          </div>
        </section>
      ) : null}
    </div>
  );
}
