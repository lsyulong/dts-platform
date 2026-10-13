import { useId } from 'react';

function localDateTime(value: unknown) {
  if (typeof value !== 'string' || !value) return '';
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return '';
  const pad = (part: number) => String(part).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export default function ExpiryInput({
  label,
  value,
  onChange,
  disabled,
  maximum,
  description,
}: {
  label: string;
  value: unknown;
  onChange: (value: string | undefined) => void;
  disabled?: boolean;
  maximum?: string;
  description?: string;
}) {
  const id = useId();
  return (
    <div>
      <label
        htmlFor={id}
        className="mb-1.5 block text-sm font-medium text-[var(--color-text-secondary)]"
      >
        {label}
      </label>
      <input
        id={id}
        type="datetime-local"
        value={localDateTime(value)}
        max={localDateTime(maximum)}
        disabled={disabled}
        className="w-full rounded-md border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm"
        onChange={(event) => {
          const raw = event.target.value;
          const date = new Date(raw);
          onChange(raw && Number.isFinite(date.getTime()) ? date.toISOString() : undefined);
        }}
      />
      {description && (
        <p className="mt-1.5 text-xs text-[var(--color-text-tertiary)]">{description}</p>
      )}
    </div>
  );
}
