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
    neutral: 'text-zinc-400',
  };

  return (
    <div className="rounded-2xl border border-zinc-800/60 bg-zinc-900/40 p-5 backdrop-blur-sm transition-all hover:border-zinc-700/60">
      <div className="flex items-start justify-between">
        <p className="text-xs font-medium text-zinc-500">{title}</p>
        {icon && <div className="text-zinc-500">{icon}</div>}
      </div>
      <div className="mt-3 flex items-baseline gap-1.5">
        <span className="text-2xl font-bold text-zinc-100">{value}</span>
        {unit && <span className="text-sm text-zinc-500">{unit}</span>}
      </div>
      {subtitle && (
        <p className={`mt-1 text-xs ${trend ? trendColors[trend] : 'text-zinc-500'}`}>
          {subtitle}
        </p>
      )}
    </div>
  );
}
