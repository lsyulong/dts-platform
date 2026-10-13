import type { ReactNode } from 'react';
import { Plus, Trash2 } from 'lucide-react';

export function EditorHeader({
  title,
  addLabel,
  onAdd,
  disabled,
}: {
  title: string;
  addLabel: string;
  onAdd: () => void;
  disabled?: boolean;
}) {
  return (
    <div className="structured-editor-header">
      <strong>{title}</strong>
      <button type="button" className="structured-editor-add" onClick={onAdd} disabled={disabled}>
        <Plus size={15} aria-hidden="true" />
        {addLabel}
      </button>
    </div>
  );
}

export function RemoveButton({
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
      className="structured-editor-remove"
      aria-label={label}
      title={label}
      onClick={onRemove}
      disabled={disabled}
    >
      <Trash2 size={15} aria-hidden="true" />
    </button>
  );
}

export function EditorWarning({ children }: { children: ReactNode }) {
  return (
    <p role="alert" className="structured-editor-warning">
      {children}
    </p>
  );
}
