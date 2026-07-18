import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Plus, Search, Play, Pause } from 'lucide-react';
import { ErrorBanner, EmptyState, StatusBadge } from '../../components/ui';
import { TaskListSkeleton } from '../../components/skeletons';
import { useTasks, useUpdateTaskStatus, type Task } from '../../hooks/useApi';

export default function TaskListPage() {
  const { t } = useTranslation();
  const { data: tasks = [], isLoading, error } = useTasks();
  const updateStatus = useUpdateTaskStatus();
  const [search, setSearch] = useState('');

  const fetchError = error ? t('tasks.loadError') : null;

  const filteredTasks = (tasks as Task[]).filter(
    (t) =>
      t.name.toLowerCase().includes(search.toLowerCase()) ||
      (t.description || '').toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold text-[var(--color-text-primary)]">{t('tasks.title')}</h1>
          <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">{t('tasks.subtitle')}</p>
        </div>
        <Link
          to="/tasks/create"
          className="flex items-center gap-2 rounded-xl bg-blue-500 px-4 py-2.5 text-sm font-medium text-white shadow-[0_0_20px_rgba(37,99,235,0.2)] transition-all hover:bg-blue-600"
        >
          <Plus size={16} />
          {t('tasks.createTask')}
        </Link>
      </div>

      {/* Error banner */}
      {fetchError && <ErrorBanner message={fetchError} />}

      {/* Search */}
      <div className="relative">
        <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-[var(--color-text-tertiary)]" />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={t('tasks.searchPlaceholder')}
          className="w-full rounded-xl border border-[var(--color-border-hover)] bg-[var(--color-bg-tertiary)] py-2.5 pl-10 pr-4 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-disabled)] focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>

      {/* Task list */}
      {isLoading ? (
        <TaskListSkeleton />
      ) : filteredTasks.length === 0 ? (
        <EmptyState
          title={t('tasks.noTasks')}
          action={
            <Link
              to="/tasks/create"
              className="text-sm text-blue-400 hover:text-blue-300"
            >
              {t('tasks.createFirst')} →
            </Link>
          }
        />
      ) : (
        <div className="space-y-3">
          {filteredTasks.map((task) => (
            <div
              key={task.id}
              className="group flex items-center justify-between rounded-2xl border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-4 transition-all hover:border-[var(--color-border-hover)]"
            >
              <div className="flex-1">
                <Link
                  to={`/tasks/${task.id}`}
                  className="text-sm font-medium text-[var(--color-text-primary)] hover:text-blue-400"
                >
                  {task.name}
                </Link>
                <div className="mt-1 flex items-center gap-3">
                  <StatusBadge status={task.status} />
                  {task.cron_expr && (
                    <span className="text-xs text-[var(--color-text-disabled)]">cron: {task.cron_expr}</span>
                  )}
                </div>
                {task.description && (
                  <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">{task.description}</p>
                )}
              </div>

              {/* Action buttons (visible on hover) */}
              <div className="flex items-center gap-2 opacity-0 transition-opacity group-hover:opacity-100">
                {task.status === 'draft' || task.status === 'paused' ? (
                  <button
                    onClick={() => updateStatus.mutate({ id: task.id, status: 'running' })}
                    className="rounded-lg p-2 text-emerald-400 hover:bg-emerald-500/10"
                    title={t('tasks.detail.start')}
                  >
                    <Play size={16} />
                  </button>
                ) : null}
                {task.status === 'running' ? (
                  <button
                    onClick={() => updateStatus.mutate({ id: task.id, status: 'paused' })}
                    className="rounded-lg p-2 text-amber-400 hover:bg-amber-500/10"
                    title={t('tasks.detail.pause')}
                  >
                    <Pause size={16} />
                  </button>
                ) : null}
                <Link
                  to={`/tasks/${task.id}`}
                  className="rounded-lg px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                >
                  {t('common.view')}
                </Link>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
