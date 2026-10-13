import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Plus, Search, Play, Square } from 'lucide-react';
import { ErrorBanner, EmptyState, StatusBadge } from '../../components/ui';
import { TaskListSkeleton } from '../../components/skeletons';
import { useTasks, useUpdateTaskStatus } from '../../hooks/useApi';
import { apiErrorMessage } from '../../lib/apiError';
import { useAuthStore } from '../../stores/auth';

export default function TaskListPage() {
  const { t } = useTranslation();
  const { data: tasks = [], isLoading, error } = useTasks();
  const updateStatus = useUpdateTaskStatus();
  const user = useAuthStore((state) => state.user);
  const canWrite = user?.permissions.includes('task:write') ?? false;
  const canExecute = user?.permissions.includes('task:execute') ?? false;
  const [search, setSearch] = useState('');

  const fetchError = error ? t('tasks.loadError') : null;

  const filteredTasks = tasks.filter(
    (t) =>
      t.name.toLowerCase().includes(search.toLowerCase()) ||
      (t.description || '').toLowerCase().includes(search.toLowerCase()),
  );

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-bold text-[var(--color-text-primary)]">{t('tasks.title')}</h1>
          <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">{t('tasks.subtitle')}</p>
        </div>
        {canWrite && (
          <Link
            to="/tasks/create"
            className="inline-flex shrink-0 items-center gap-2 rounded-lg bg-[var(--color-action)] px-4 py-2.5 text-sm font-medium text-white hover:bg-[var(--color-action-hover)]"
          >
            <Plus size={16} />
            {t('tasks.createTask')}
          </Link>
        )}
      </div>

      {fetchError && <ErrorBanner message={fetchError} />}
      {updateStatus.error && (
        <ErrorBanner
          message={`${t('tasks.updateStatusError')}: ${apiErrorMessage(updateStatus.error, t('errors.tryAgainLater'))}`}
        />
      )}

      <div className="relative max-w-lg">
        <Search
          size={16}
          className="absolute left-3 top-1/2 -translate-y-1/2 text-[var(--color-text-tertiary)]"
        />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={t('tasks.searchPlaceholder')}
          aria-label={t('common.search')}
          className="w-full rounded-lg border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] py-2.5 pl-10 pr-4 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-tertiary)] focus:border-[var(--color-accent)] focus:outline-none focus:ring-1 focus:ring-[var(--color-accent)]"
        />
      </div>

      {isLoading ? (
        <TaskListSkeleton />
      ) : filteredTasks.length === 0 ? (
        <EmptyState
          title={t('tasks.noTasks')}
          action={
            canWrite ? (
              <Link
                to="/tasks/create"
                className="text-sm text-[var(--color-accent)] hover:text-[var(--color-accent-hover)]"
              >
                {t('tasks.createFirst')} →
              </Link>
            ) : undefined
          }
        />
      ) : (
        <div className="data-panel overflow-hidden">
          <div className="overflow-x-auto">
            <table className="data-table min-w-[720px]" aria-label={t('tasks.title')}>
              <thead>
                <tr>
                  <th scope="col">{t('tasks.taskName')}</th>
                  <th scope="col">{t('tasks.detail.status')}</th>
                  <th scope="col">{t('tasks.schedule')}</th>
                  <th scope="col" className="text-right">
                    {t('common.actions')}
                  </th>
                </tr>
              </thead>
              <tbody>
                {filteredTasks.map((task) => (
                  <tr key={task.id}>
                    <td className="min-w-56">
                      <Link
                        to={`/tasks/${task.id}`}
                        className="font-medium text-[var(--color-text-primary)] hover:text-[var(--color-accent)]"
                      >
                        {task.name}
                      </Link>
                      {task.description && (
                        <p className="mt-1 max-w-md text-xs text-[var(--color-text-tertiary)]">
                          {task.description}
                        </p>
                      )}
                    </td>
                    <td className="whitespace-nowrap">
                      <StatusBadge status={task.status} />
                    </td>
                    <td className="whitespace-nowrap text-xs">
                      {task.cron_expr ? `cron: ${task.cron_expr}` : t('tasks.manualTrigger')}
                    </td>
                    <td>
                      <div className="flex items-center justify-end gap-1 whitespace-nowrap">
                        {canExecute && task.status !== 'running' ? (
                          <button
                            type="button"
                            onClick={() => updateStatus.mutate({ id: task.id, status: 'running' })}
                            disabled={updateStatus.isPending}
                            className="inline-flex items-center gap-1.5 rounded-lg px-3 py-2 text-xs text-[var(--color-accent)] hover:bg-[var(--color-accent-subtle)] disabled:opacity-50"
                            title={
                              task.status === 'draft'
                                ? t('tasks.detail.start')
                                : t('tasks.detail.rerun')
                            }
                          >
                            <Play size={14} aria-hidden="true" />
                            {task.status === 'draft'
                              ? t('tasks.detail.start')
                              : t('tasks.detail.rerun')}
                          </button>
                        ) : null}
                        {canExecute && task.status === 'running' ? (
                          <button
                            type="button"
                            onClick={() =>
                              updateStatus.mutate({ id: task.id, status: 'cancelled' })
                            }
                            disabled={
                              updateStatus.isPending ||
                              (updateStatus.isSuccess &&
                                updateStatus.variables?.status === 'cancelled' &&
                                updateStatus.variables.id === task.id)
                            }
                            className="inline-flex items-center gap-1.5 rounded-lg px-3 py-2 text-xs text-[var(--color-warning)] hover:bg-[var(--color-warning-subtle)] disabled:opacity-50"
                            title={t(
                              updateStatus.isSuccess &&
                                updateStatus.variables?.status === 'cancelled' &&
                                updateStatus.variables.id === task.id
                                ? 'tasks.detail.cancelling'
                                : 'tasks.detail.cancelRun',
                            )}
                          >
                            <Square size={14} aria-hidden="true" />
                            {t(
                              updateStatus.isSuccess &&
                                updateStatus.variables?.status === 'cancelled' &&
                                updateStatus.variables.id === task.id
                                ? 'tasks.detail.cancelling'
                                : 'tasks.detail.cancelRun',
                            )}
                          </button>
                        ) : null}
                        <Link
                          to={`/tasks/${task.id}`}
                          className="rounded-lg px-3 py-2 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                        >
                          {t('common.view')}
                        </Link>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}
