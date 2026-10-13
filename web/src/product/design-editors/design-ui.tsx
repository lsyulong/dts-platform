import { Button } from '../../components/ui/button';

export function DesignHeader({
  title,
  button,
  onAdd,
  disabled,
}: {
  title: string;
  button?: string;
  onAdd?: () => void;
  disabled?: boolean;
}) {
  return (
    <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
      <strong className="text-sm font-semibold text-[var(--color-text-primary)]">{title}</strong>
      {button && onAdd && (
        <Button type="button" size="sm" variant="outline" onClick={onAdd} disabled={disabled}>
          {button}
        </Button>
      )}
    </div>
  );
}
export function DesignWarning({ children }: { children: React.ReactNode }) {
  return (
    <p
      role="alert"
      className="mt-3 rounded-md border border-[var(--color-error)]/20 bg-[var(--color-error-subtle)] p-3 text-xs text-[var(--color-error)]"
    >
      {children}
    </p>
  );
}
