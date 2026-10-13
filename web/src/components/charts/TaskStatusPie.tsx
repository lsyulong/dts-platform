import { useTranslation } from 'react-i18next';
import { PieChart, Pie, Cell, Tooltip, ResponsiveContainer, Legend } from 'recharts';

const STATUS_COLORS: Record<string, string> = {
  running: 'var(--color-accent)',
  completed: '#10b981',
  failed: '#f43f5e',
  draft: '#a1a1aa',
  paused: '#f59e0b',
};

type Props = {
  tasks: Array<{ status: string }>;
};

export function TaskStatusPie({ tasks }: Props) {
  const { t } = useTranslation();
  if (tasks.length === 0)
    return (
      <div className="flex h-64 flex-col">
        <h4 className="mb-3 text-xs font-medium text-[var(--color-text-tertiary)]">
          {t('dashboard.taskDistribution')}
        </h4>
        <p
          role="status"
          className="flex flex-1 items-center justify-center text-sm text-[var(--color-text-tertiary)]"
        >
          {t('dashboard.noTaskDistribution')}
        </p>
      </div>
    );

  const grouped = tasks.reduce<Record<string, number>>((acc, t) => {
    acc[t.status] = (acc[t.status] || 0) + 1;
    return acc;
  }, {});
  const data = Object.entries(grouped).map(([rawStatus, value]) => ({
    rawStatus,
    name: t(`tasks.status.${rawStatus}`),
    value,
  }));

  return (
    <div className="h-64">
      <h4 className="mb-3 text-xs font-medium text-[var(--color-text-tertiary)]">
        {t('dashboard.taskDistribution')}
      </h4>
      <ResponsiveContainer width="100%" height="100%">
        <PieChart>
          <Pie
            data={data}
            dataKey="value"
            nameKey="name"
            cx="50%"
            cy="50%"
            outerRadius={70}
            label={({ name, percent }) => `${name} ${((percent ?? 0) * 100).toFixed(0)}%`}
            labelLine={false}
          >
            {data.map((entry) => (
              <Cell key={entry.name} fill={STATUS_COLORS[entry.rawStatus] ?? '#52525b'} />
            ))}
          </Pie>
          <Tooltip
            contentStyle={{
              backgroundColor: 'var(--color-bg-secondary)',
              border: '1px solid var(--color-border-default)',
              borderRadius: '12px',
              fontSize: '12px',
            }}
          />
          <Legend wrapperStyle={{ fontSize: '11px' }} iconType="circle" iconSize={8} />
        </PieChart>
      </ResponsiveContainer>
    </div>
  );
}
