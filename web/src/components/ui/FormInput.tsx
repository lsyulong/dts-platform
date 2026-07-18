type Props = {
  label?: string;
  error?: string;
  type?: 'text' | 'password' | 'number' | 'email' | 'textarea';
  placeholder?: string;
  value?: string | number;
  onChange?: (value: string) => void;
  required?: boolean;
  disabled?: boolean;
  className?: string;
  rows?: number;
};

function buildInputClass(hasError: boolean): string {
  const base =
    'w-full rounded-xl border bg-[var(--color-bg-tertiary)] px-4 py-2.5 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-disabled)] focus:outline-none focus:ring-1 transition-colors';
  const borderColor = hasError
    ? 'border-red-500/50 focus:border-red-500 focus:ring-red-500'
    : 'border-[var(--color-border-hover)] focus:border-blue-500 focus:ring-blue-500';
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
}: Props) {
  const inputClass = `${buildInputClass(!!error)} ${className}`;

  return (
    <div>
      {label && (
        <label className="mb-1.5 block text-sm font-medium text-[var(--color-text-secondary)]">
          {label}
          {required && <span className="ml-1 text-red-400">*</span>}
        </label>
      )}
      {type === 'textarea' ? (
        <textarea
          value={value}
          onChange={(e) => onChange?.(e.target.value)}
          placeholder={placeholder}
          rows={rows}
          disabled={disabled}
          className={inputClass}
        />
      ) : (
        <input
          type={type}
          value={value}
          onChange={(e) => onChange?.(e.target.value)}
          placeholder={placeholder}
          required={required}
          disabled={disabled}
          className={inputClass}
        />
      )}
      {error && <p className="mt-1 text-xs text-red-400">{error}</p>}
    </div>
  );
}
