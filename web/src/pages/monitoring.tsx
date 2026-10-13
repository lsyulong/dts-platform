import { useTranslation } from 'react-i18next';
import { Activity, Zap, Globe, Server } from 'lucide-react';
import MetricCard from '../components/MetricCard';
import LogConsole from '../components/LogConsole';
import { LoadingSpinner, ErrorBanner } from '../components/ui';
import { useAlerts, useToggleAlert } from '../hooks/useApi';

export default function MonitoringPage() {
  const { t } = useTranslation();
  const { data: alerts = [], isLoading, error } = useAlerts();
  const toggleAlert = useToggleAlert();

  const fetchError = error ? t('errors.somethingWentWrong') : null;

  const metricLabel: Record<string, string> = {
    latency: t('metrics.latency'),
    error_rate: t('metrics.error_rate'),
    throughput: t('metrics.throughput'),
    row_count: t('metrics.row_count'),
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-bold text-[var(--color-text-primary)]">
          {t('monitoring.title')}
        </h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">{t('monitoring.subtitle')}</p>
      </div>

      {/* Error banner */}
      {fetchError && <ErrorBanner message={fetchError} />}

      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <MetricCard
          title={t('monitoring.cpuUsage')}
          value="--"
          subtitle={t('monitoring.notCollected')}
          icon={<Server size={18} />}
        />
        <MetricCard
          title={t('monitoring.memory')}
          value="--"
          subtitle={t('monitoring.notCollected')}
          icon={<Activity size={18} />}
        />
        <MetricCard
          title={t('monitoring.networkIO')}
          value="--"
          subtitle={t('monitoring.notCollected')}
          icon={<Globe size={18} />}
        />
        <MetricCard
          title={t('monitoring.avgLatency')}
          value="--"
          subtitle={t('monitoring.notCollected')}
          icon={<Zap size={18} />}
        />
      </div>

      {/* Alert rules */}
      <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5 backdrop-blur-sm">
        <h3 className="mb-4 text-sm font-medium text-[var(--color-text-secondary)]">
          {t('monitoring.alertRules')}
        </h3>
        {isLoading ? (
          <LoadingSpinner size="sm" />
        ) : alerts.length === 0 ? (
          <p className="text-sm text-[var(--color-text-disabled)]">{t('tasks.noTasks')}</p>
        ) : (
          <div className="space-y-2">
            {alerts.map((alert) => (
              <div
                key={alert.id}
                className="flex items-center justify-between rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-bg-secondary)] p-3"
              >
                <div>
                  <p className="text-sm font-medium text-[var(--color-text-primary)]">
                    {alert.name}
                  </p>
                  <p className="text-xs text-[var(--color-text-tertiary)]">
                    {metricLabel[alert.metric_type] || alert.metric_type} {alert.operator}{' '}
                    {alert.threshold}
                  </p>
                </div>
                <button
                  onClick={() => toggleAlert.mutate(alert.id)}
                  className={`relative h-6 w-11 rounded-full transition-colors ${
                    alert.enabled ? 'bg-[var(--color-action)]' : 'bg-[var(--color-bg-tertiary)]'
                  }`}
                >
                  <div
                    className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-transform ${
                      alert.enabled ? 'translate-x-5.5' : 'translate-x-0.5'
                    }`}
                    style={{ left: alert.enabled ? undefined : 0 }}
                  />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Log console */}
      <LogConsole title={t('dashboard.globalLog')} />
    </div>
  );
}
