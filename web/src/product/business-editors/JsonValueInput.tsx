import { useState } from 'react';
import { inputClass } from './business-ui';

export default function JsonValueInput({
  id,
  value,
  onChange,
  required,
  disabled,
}: {
  id: string;
  value: unknown;
  onChange: (value: unknown) => void;
  required: boolean;
  disabled?: boolean;
}) {
  const [source, setSource] = useState(value === undefined ? '' : JSON.stringify(value, null, 2));
  return (
    <textarea
      id={id}
      className={inputClass}
      rows={4}
      spellCheck={false}
      required={required}
      disabled={disabled}
      value={source}
      onChange={(event) => {
        const next = event.target.value;
        setSource(next);
        try {
          const parsed: unknown = JSON.parse(next);
          event.target.setCustomValidity('');
          onChange(parsed);
        } catch {
          event.target.setCustomValidity('JSON');
        }
      }}
    />
  );
}
