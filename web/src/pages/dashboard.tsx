import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Database, CheckCircle, Activity } from 'lucide-react';
import MetricCard from '../components/MetricCard';
import LogConsole from '../components/LogConsole';
import { ErrorBanner, StatusBadge } from '../components/ui';
import { DashboardSkeleton } from '../components/skeletons';
import { SyncVolumeChart, TaskStatusPie } from '../components/charts';
import { useOverview, useTasks } from '../hooks/useApi';

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toString();
}

export default function DashboardPage() {
  const { t } = useTranslation();
  const { data: overview, isLoading, error } = useOverview();
  const { data: tasks = [] } = useTasks();
  const hasTasks = (overview?.total_tasks ?? 0) > 0;
  const hasDatasources = (overview?.total_datasources ?? 0) > 0;
  const connectionsConfirmed =
    hasDatasources && overview?.healthy_datasources === overview?.total_datasources;

  const fetchError = error ? t('errors.somethingWentWrong') : null;

  if (isLoading) {
    return <DashboardSkeleton />;
  }

  if (fetchError) {
    return (
      <div className="py-20">
        <ErrorBanner message={fetchError} onRetry={() => window.location.reload()} />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Page header */}
      <div>
        <h1 className="text-xl font-bold text-[var(--color-text-primary)]">
          {t('dashboard.title')}
        </h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">{t('dashboard.subtitle')}</p>
      </div>

      {/* KPI cards */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <MetricCard
          title={t('dashboard.totalTasks')}
          value={overview?.total_tasks ?? 0}
          subtitle={t('dashboard.runningCount', { count: overview?.running_tasks ?? 0 })}
          icon={<Activity size={18} />}
        />
        <MetricCard
          title={t('dashboard.todaySyncData')}
          value={formatBytes(overview?.today_synced_bytes ?? 0)}
          subtitle={t('dashboard.rowsCount', {
            count: formatNumber(overview?.today_synced_rows ?? 0),
          })}
          trend="up"
        />
        <MetricCard
          title={t('dashboard.successRate')}
          value={hasTasks ? `${(overview?.success_rate ?? 0).toFixed(1)}%` : '--'}
          subtitle={
            hasTasks
              ? t('dashboard.failedCount', { count: overview?.failed_tasks ?? 0 })
              : t('dashboard.noData')
          }
          trend={hasTasks ? (overview && overview.failed_tasks > 0 ? 'down' : 'up') : 'neutral'}
          icon={<CheckCircle size={18} />}
        />
        <MetricCard
          title={t('dashboard.healthyDatasources')}
          value={
            hasDatasources
              ? `${overview?.healthy_datasources ?? 0}/${overview?.total_datasources ?? 0}`
              : '--'
          }
          subtitle={
            !hasDatasources
              ? t('datasources.noDatasources')
              : t(
                  connectionsConfirmed
                    ? 'dashboard.connectionHealthy'
                    : 'dashboard.connectionUnconfirmed',
                )
          }
          icon={<Database size={18} />}
        />
      </div>

      {/* Recent runs + Charts */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* Recent task runs */}
        <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5 backdrop-blur-sm">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-[var(--color-text-secondary)]">
              {t('dashboard.recentRuns')}
            </h3>
            <Link
              to="/tasks"
              className="text-xs text-[var(--color-accent)] hover:text-[var(--color-accent-hover)]"
            >
              {t('dashboard.viewAll')}
            </Link>
          </div>
          <div className="mt-4 space-y-3">
            {overview?.recent_runs?.length === 0 ? (
              <p className="text-sm text-[var(--color-text-disabled)]">{t('dashboard.noRuns')}</p>
            ) : (
              overview?.recent_runs?.slice(0, 5).map((run) => (
                <div
                  key={run.id}
                  className="flex items-center justify-between rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-bg-secondary)] p-3"
                >
                  <div>
                    <Link
                      to={`/tasks/${run.task_id}`}
                      className="text-sm font-medium text-[var(--color-text-primary)] hover:text-[var(--color-accent-hover)]"
                    >
                      {run.task_name}
                    </Link>
                    <p className="text-xs text-[var(--color-text-tertiary)]">{run.start_time}</p>
                  </div>
                  <div className="text-right">
                    <StatusBadge status={run.status} />
                    <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                      {t('dashboard.rowsCount', { count: formatNumber(run.rows_synced) })}
                    </p>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>

        {/* Charts */}
        <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5 backdrop-blur-sm">
          <SyncVolumeChart
            runs={
              overview?.recent_runs?.map((r) => ({
                task_name: r.task_name,
                rows_synced: r.rows_synced,
              })) ?? []
            }
          />
        </div>
      </div>

      {/* Task distribution + Log */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5 backdrop-blur-sm">
          <TaskStatusPie tasks={tasks} />
        </div>
        <LogConsole title={t('dashboard.systemLog')} />
      </div>
    </div>
  );
}
