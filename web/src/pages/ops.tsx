import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ExternalLink, RefreshCw, Clock, Play, FileText, Monitor } from 'lucide-react';
import { useTasks, useScheduleStatus } from '../hooks/useApi';
import { LoadingSpinner, EmptyState } from '../components/ui';
import { apiErrorMessage } from '../lib/apiError';

const JIASCHEDULER_URL = 'http://localhost:9090';

type Tab = 'instances' | 'dispatch' | 'logs' | 'agents';

export default function OpsCenterPage() {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<Tab>('instances');
  const { data: tasks = [], isLoading } = useTasks();

  const tabs: { key: Tab; label: string; icon: React.ReactNode }[] = [
    { key: 'instances', label: t('ops.instances'), icon: <Clock size={14} /> },
    { key: 'dispatch', label: t('ops.dispatch'), icon: <Play size={14} /> },
    { key: 'logs', label: t('ops.logs'), icon: <FileText size={14} /> },
    { key: 'agents', label: t('ops.agents'), icon: <Monitor size={14} /> },
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold text-[var(--color-text-primary)]">{t('nav.ops')}</h1>
          <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">{t('ops.subtitle')}</p>
        </div>
        <a
          href={JIASCHEDULER_URL}
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center gap-2 rounded-lg border border-[var(--color-border-hover)] px-4 py-2.5 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
        >
          <ExternalLink size={14} />
          jiascheduler {t('ops.console')}
        </a>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-4 gap-4">
        {[
          {
            label: t('ops.totalTasks'),
            value: tasks.length,
            color: 'text-[var(--color-text-primary)]',
          },
          {
            label: t('ops.runningTasks'),
            value: tasks.filter((t) => t.status === 'running').length,
            color: 'text-[var(--color-accent)]',
          },
          {
            label: t('ops.failedTasks'),
            value: tasks.filter((t) => t.status === 'failed').length,
            color: 'text-[var(--color-error)]',
          },
          { label: t('ops.activeSchedules'), value: '--', color: 'text-[var(--color-success)]' },
        ].map((stat, i) => (
          <div
            key={i}
            className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-4"
          >
            <p className="text-xs text-[var(--color-text-tertiary)]">{stat.label}</p>
            <p className={`mt-1 text-2xl font-bold ${stat.color}`}>{stat.value}</p>
          </div>
        ))}
      </div>

      {/* Tabs */}
      <div className="flex items-center gap-2 border-b border-[var(--color-border-default)]">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key)}
            className={`flex items-center gap-2 border-b-2 px-4 py-3 text-sm font-medium transition-colors -mb-px ${
              activeTab === tab.key
                ? 'border-[var(--color-accent)] text-[var(--color-accent)]'
                : 'border-transparent text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)]'
            }`}
          >
            {tab.icon}
            {tab.label}
          </button>
        ))}
      </div>

      {/* Tab Content */}
      {isLoading ? (
        <LoadingSpinner />
      ) : (
        <div>
          {activeTab === 'instances' && <InstancesTab tasks={tasks} />}
          {activeTab === 'dispatch' && <DispatchTab tasks={tasks} />}
          {activeTab === 'logs' && <LogsTab tasks={tasks} />}
          {activeTab === 'agents' && <AgentsTab />}
        </div>
      )}
    </div>
  );
}

// ── Tab Contents ──────────────────────────────────────────────────

function InstancesTab({ tasks }: { tasks: Array<{ id: string; name: string; status: string }> }) {
  const { t } = useTranslation();

  return (
    <div className="mt-4 space-y-3">
      <div className="flex items-center gap-2 text-xs text-[var(--color-text-tertiary)]">
        <RefreshCw size={12} />
        {t('ops.instancesHint')}
      </div>
      {tasks.length === 0 ? (
        <EmptyState title={t('ops.noInstances')} />
      ) : (
        <div className="space-y-2">
          {tasks.map((task) => (
            <InstanceRow key={task.id} taskId={task.id} taskName={task.name} />
          ))}
        </div>
      )}
    </div>
  );
}

function InstanceRow({ taskId, taskName }: { taskId: string; taskName: string }) {
  const { t } = useTranslation();
  const { data: schedule, isLoading, error } = useScheduleStatus(taskId);

  return (
    <div className="flex items-center justify-between rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-bg-secondary)] p-3">
      <div className="flex-1">
        <p className="text-sm font-medium text-[var(--color-text-primary)]">{taskName}</p>
        <p className="text-xs text-[var(--color-text-tertiary)]">ID: {taskId.slice(0, 8)}...</p>
      </div>
      <div className="flex items-center gap-4">
        {isLoading ? (
          <span className="text-xs text-[var(--color-text-disabled)]">{t('common.loading')}</span>
        ) : error ? (
          <span role="alert" className="text-xs text-[var(--color-error)]">
            {apiErrorMessage(error, t('tasks.detail.scheduleError'))}
          </span>
        ) : schedule?.enabled === false ? (
          <span className="text-xs text-[var(--color-text-tertiary)]">
            {t('tasks.detail.schedulerUnavailable')}
          </span>
        ) : schedule?.has_timer ? (
          <div className="text-right">
            <span
              className={`inline-block rounded-full px-2 py-0.5 text-xs font-medium ${schedule.active ? 'bg-emerald-500/10 text-[var(--color-success)]' : 'bg-amber-500/10 text-[var(--color-warning)]'}`}
            >
              {t(
                `ops.scheduleStates.${schedule.active ? 'active' : (schedule.state ?? 'inactive')}`,
              )}
            </span>
            <p className="mt-0.5 text-xs text-[var(--color-text-tertiary)]">
              {schedule.cron_expr} (UTC) · {t('ops.dispatchHistory')} (
              {schedule.schedule_history.length})
            </p>
          </div>
        ) : (
          <span className="text-xs text-[var(--color-text-disabled)]">{t('ops.notScheduled')}</span>
        )}
        <a
          href={`${JIASCHEDULER_URL}/exec/query?eid=dts-task-${taskId}`}
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center gap-1 rounded-lg px-2 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
        >
          {t('common.view')} <ExternalLink size={10} />
        </a>
      </div>
    </div>
  );
}

function DispatchTab({ tasks: _tasks }: { tasks: Array<{ id: string; name: string }> }) {
  const { t } = useTranslation();
  return (
    <div className="mt-4 space-y-4">
      <p className="text-sm text-[var(--color-text-tertiary)]">{t('ops.dispatchHint')}</p>
      <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5">
        <h3 className="mb-3 text-sm font-medium text-[var(--color-text-secondary)]">
          {t('ops.quickDispatch')}
        </h3>
        <p className="text-xs text-[var(--color-text-disabled)] mb-4">{t('ops.dispatchDesc')}</p>
        <a
          href={`${JIASCHEDULER_URL}/dispatch`}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-2 rounded-lg bg-[var(--color-action)] px-4 py-2.5 text-sm font-medium text-white hover:bg-[var(--color-action-hover)]"
        >
          {t('ops.gotoJiascheduler')} <ExternalLink size={14} />
        </a>
      </div>
    </div>
  );
}

function LogsTab({ tasks: _tasks }: { tasks: Array<{ id: string; name: string }> }) {
  const { t } = useTranslation();
  return (
    <div className="mt-4 space-y-4">
      <p className="text-sm text-[var(--color-text-tertiary)]">{t('ops.logsHint')}</p>
      <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5">
        <h3 className="mb-3 text-sm font-medium text-[var(--color-text-secondary)]">
          {t('ops.execLogs')}
        </h3>
        <p className="text-xs text-[var(--color-text-disabled)] mb-4">{t('ops.logsDesc')}</p>
        <a
          href={`${JIASCHEDULER_URL}/exec/query`}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-2 rounded-lg bg-[var(--color-action)] px-4 py-2.5 text-sm font-medium text-white hover:bg-[var(--color-action-hover)]"
        >
          {t('ops.gotoJiascheduler')} <ExternalLink size={14} />
        </a>
      </div>
    </div>
  );
}

function AgentsTab() {
  const { t } = useTranslation();
  return (
    <div className="mt-4 space-y-4">
      <p className="text-sm text-[var(--color-text-tertiary)]">{t('ops.agentsHint')}</p>
      <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5">
        <h3 className="mb-3 text-sm font-medium text-[var(--color-text-secondary)]">
          {t('ops.agentManagement')}
        </h3>
        <p className="text-xs text-[var(--color-text-disabled)] mb-4">{t('ops.agentsDesc')}</p>
        <a
          href={`${JIASCHEDULER_URL}/instance`}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-2 rounded-lg bg-[var(--color-action)] px-4 py-2.5 text-sm font-medium text-white hover:bg-[var(--color-action-hover)]"
        >
          {t('ops.gotoJiascheduler')} <ExternalLink size={14} />
        </a>
      </div>
    </div>
  );
}
