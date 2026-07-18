import { useTranslation } from 'react-i18next';

type Props = {
  status: string;
  label?: string;
  size?: 'sm' | 'md';
};

const STATUS_COLORS: Record<string, string> = {
  draft: 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)]',
  running: 'bg-blue-500/10 text-blue-400',
  paused: 'bg-amber-500/10 text-amber-400',
  completed: 'bg-emerald-500/10 text-emerald-400',
  failed: 'bg-rose-500/10 text-rose-400',
  cancelled: 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)]',
  healthy: 'bg-emerald-500/10 text-emerald-400',
  unhealthy: 'bg-rose-500/10 text-rose-400',
};

export function StatusBadge({ status, label, size = 'sm' }: Props) {
  const { t } = useTranslation();
  const colorClass = STATUS_COLORS[status] || 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)]';
  const text = label ?? t(`tasks.status.${status}`);
  const sizeClass = size === 'md' ? 'px-2.5 py-1 text-sm' : 'px-2 py-0.5 text-xs';

  return (
    <span className={`inline-block rounded-full font-medium ${sizeClass} ${colorClass}`}>
      {text}
    </span>
  );
}
