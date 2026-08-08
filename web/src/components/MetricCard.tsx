interface MetricCardProps {
  title: string;
  value: string | number;
  unit?: string;
  subtitle?: string;
  trend?: 'up' | 'down' | 'neutral';
  icon?: React.ReactNode;
}

export default function MetricCard({ title, value, unit, subtitle, trend, icon }: MetricCardProps) {
  const trendColors = {
    up: 'text-emerald-400',
    down: 'text-rose-400',
    neutral: 'text-[var(--color-text-secondary)]',
  };

  return (
    <div className="rounded-2xl border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-5 backdrop-blur-sm transition-all hover:border-[var(--color-border-hover)]">
      <div className="flex items-start justify-between">
        <p className="text-xs font-medium text-[var(--color-text-tertiary)]">{title}</p>
        {icon && <div className="text-[var(--color-text-tertiary)]">{icon}</div>}
      </div>
      <div className="mt-3 flex items-baseline gap-1.5">
        <span className="text-2xl font-bold text-[var(--color-text-primary)]">{value}</span>
        {unit && <span className="text-sm text-[var(--color-text-tertiary)]">{unit}</span>}
      </div>
      {subtitle && (
        <p className={`mt-1 text-xs ${trend ? trendColors[trend] : 'text-[var(--color-text-tertiary)]'}`}>
          {subtitle}
        </p>
      )}
    </div>
  );
}
