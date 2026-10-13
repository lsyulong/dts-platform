import { useId } from 'react';

type Props = {
  label?: string;
  error?: string;
  type?: 'text' | 'password' | 'number' | 'email' | 'textarea' | 'date' | 'datetime-local';
  placeholder?: string;
  value?: string | number;
  onChange?: (value: string) => void;
  required?: boolean;
  disabled?: boolean;
  className?: string;
  rows?: number;
  min?: number;
  max?: number;
  step?: number | 'any';
};

function buildInputClass(hasError: boolean): string {
  const base =
    'w-full rounded-md border bg-[var(--color-bg-elevated)] px-3 py-2 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-disabled)] focus:outline-none focus:ring-1 transition-colors disabled:cursor-not-allowed disabled:opacity-60';
  const borderColor = hasError
    ? 'border-red-500/50 focus:border-red-500 focus:ring-red-500'
    : 'border-[var(--color-border-hover)] focus:border-[var(--color-accent)] focus:ring-[var(--color-accent)]';
  return `${base} ${borderColor}`;
}

export function FormInput({
  label,
  error,
  type = 'text',
  placeholder,
  value,
  onChange,
  required,
  disabled,
  className = '',
  rows = 3,
  min,
  max,
  step = 'any',
}: Props) {
  const id = useId();
  const errorId = `${id}-error`;
  const inputClass = `${buildInputClass(!!error)} ${className}`;

  return (
    <div>
      {label && (
        <label
          htmlFor={id}
          className="mb-1.5 block text-sm font-medium text-[var(--color-text-secondary)]"
        >
          {label}
          {required && (
            <span aria-hidden="true" className="ml-1 text-[var(--color-error)]">
              *
            </span>
          )}
        </label>
      )}
      {type === 'textarea' ? (
        <textarea
          id={id}
          aria-invalid={!!error}
          aria-describedby={error ? errorId : undefined}
          value={value}
          onChange={(e) => onChange?.(e.target.value)}
          placeholder={placeholder}
          rows={rows}
          required={required}
          disabled={disabled}
          className={inputClass}
        />
      ) : (
        <input
          id={id}
          aria-invalid={!!error}
          aria-describedby={error ? errorId : undefined}
          type={type}
          step={type === 'number' ? step : undefined}
          min={type === 'number' ? min : undefined}
          max={type === 'number' ? max : undefined}
          value={value}
          onChange={(e) => onChange?.(e.target.value)}
          placeholder={placeholder}
          required={required}
          disabled={disabled}
          className={inputClass}
        />
      )}
      {error && (
        <p id={errorId} className="mt-1 text-xs text-[var(--color-error)]">
          {error}
        </p>
      )}
    </div>
  );
}
