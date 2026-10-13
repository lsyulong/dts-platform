import type { LucideIcon } from 'lucide-react';
import { Inbox } from 'lucide-react';

type Props = {
  icon?: LucideIcon;
  title: string;
  description?: string;
  action?: React.ReactNode;
};

export function EmptyState({ icon: Icon = Inbox, title, description, action }: Props) {
  return (
    <div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-[var(--color-border-hover)] bg-[var(--color-bg-secondary)]/30 py-16 text-center">
      <Icon size={40} className="mx-auto text-[var(--color-text-disabled)]" />
      <p className="mt-4 text-sm text-[var(--color-text-tertiary)]">{title}</p>
      {description && (
        <p className="mt-1 text-xs text-[var(--color-text-disabled)]">{description}</p>
      )}
      {action && <div className="mt-3">{action}</div>}
    </div>
  );
}
