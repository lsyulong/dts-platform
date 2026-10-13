import { useTranslation } from 'react-i18next';

type Props = {
  status: string;
  label?: string;
  size?: 'sm' | 'md';
};

const STATUS_COLORS: Record<string, string> = {
  draft: 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)]',
  running: 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]',
  paused: 'bg-amber-500/10 text-[var(--color-warning)]',
  completed: 'bg-emerald-500/10 text-[var(--color-success)]',
  failed: 'bg-rose-500/10 text-[var(--color-error)]',
  cancelled: 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)]',
  healthy: 'bg-emerald-500/10 text-[var(--color-success)]',
  unhealthy: 'bg-rose-500/10 text-[var(--color-error)]',
  active: 'bg-[var(--color-success-subtle)] text-[var(--color-success)]',
  disabled: 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)]',
};

export function StatusBadge({ status, label, size = 'sm' }: Props) {
  const { t } = useTranslation();
  const colorClass =
    STATUS_COLORS[status] || 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)]';
  const text =
    label ??
    t(
      status === 'active' || status === 'disabled'
        ? `system.statusLabels.${status}`
        : `tasks.status.${status}`,
    );
  const sizeClass = size === 'md' ? 'px-2.5 py-1 text-sm' : 'px-2 py-0.5 text-xs';

  return (
    <span className={`inline-block rounded-full font-medium ${sizeClass} ${colorClass}`}>
      {text}
    </span>
  );
}
