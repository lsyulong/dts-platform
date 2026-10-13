import { useTranslation } from 'react-i18next';
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';

type Props = {
  runs: Array<{
    task_name: string;
    rows_synced: number;
  }>;
};

export function SyncVolumeChart({ runs }: Props) {
  const { t } = useTranslation();
  if (runs.length === 0)
    return (
      <div className="flex h-64 flex-col">
        <h4 className="mb-3 text-xs font-medium text-[var(--color-text-tertiary)]">
          {t('dashboard.syncVolumeChart')}
        </h4>
        <p
          role="status"
          className="flex flex-1 items-center justify-center text-sm text-[var(--color-text-tertiary)]"
        >
          {t('dashboard.noSyncVolume')}
        </p>
      </div>
    );

  return (
    <div className="h-64">
      <h4 className="mb-3 text-xs font-medium text-[var(--color-text-tertiary)]">
        {t('dashboard.syncVolumeChart')}
      </h4>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={runs} margin={{ top: 5, right: 5, left: -20, bottom: 5 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="var(--color-bg-tertiary)" />
          <XAxis
            dataKey="task_name"
            tick={{ fill: 'var(--color-text-tertiary)', fontSize: 11 }}
            axisLine={{ stroke: 'var(--color-bg-tertiary)' }}
          />
          <YAxis
            tick={{ fill: 'var(--color-text-tertiary)', fontSize: 11 }}
            axisLine={{ stroke: 'var(--color-bg-tertiary)' }}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: 'var(--color-bg-secondary)',
              border: '1px solid var(--color-border-default)',
              borderRadius: '12px',
              fontSize: '12px',
            }}
            labelStyle={{ color: 'var(--color-text-secondary)' }}
          />
          <Bar dataKey="rows_synced" fill="var(--color-accent)" radius={[4, 4, 0, 0]} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
