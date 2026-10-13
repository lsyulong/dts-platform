import type { ReactNode } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/button';

export const inputClass =
  'w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm focus:border-[var(--color-accent)] focus:outline-none disabled:opacity-50';

export function BusinessEditorFrame({
  title,
  children,
  addLabel,
  onAdd,
  disabled,
}: {
  title: string;
  children: ReactNode;
  addLabel?: string;
  onAdd?: () => void;
  disabled?: boolean;
}) {
  return (
    <div className="space-y-3 rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-secondary)] p-3 sm:p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <strong className="text-xs font-medium text-[var(--color-text-secondary)]">{title}</strong>
        {onAdd && (
          <Button type="button" size="sm" variant="outline" disabled={disabled} onClick={onAdd}>
            <Plus size={14} />
            {addLabel}
          </Button>
        )}
      </div>
      {children}
    </div>
  );
}

export function BusinessWarning({ children }: { children: ReactNode }) {
  return (
    <p role="alert" className="text-xs leading-relaxed text-[var(--color-error)]">
      {children}
    </p>
  );
}

export function BusinessRemove({
  label,
  onRemove,
  disabled,
}: {
  label: string;
  onRemove: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onRemove}
      className="rounded-md p-2 text-[var(--color-error)] transition-colors hover:bg-[var(--color-error-subtle)] disabled:opacity-40"
    >
      <Trash2 size={16} />
    </button>
  );
}

export function InvalidBusinessValue({
  text,
  resetLabel,
  onReset,
  disabled,
}: {
  text: string;
  resetLabel: string;
  onReset: () => void;
  disabled?: boolean;
}) {
  return (
    <div className="space-y-3 rounded-lg border border-[var(--color-border-default)] p-4">
      <BusinessWarning>{text}</BusinessWarning>
      <Button type="button" variant="outline" size="sm" disabled={disabled} onClick={onReset}>
        {resetLabel}
      </Button>
      <input
        aria-label={text}
        className="sr-only"
        tabIndex={-1}
        required
        value=""
        onChange={() => {}}
        disabled={disabled}
      />
    </div>
  );
}
