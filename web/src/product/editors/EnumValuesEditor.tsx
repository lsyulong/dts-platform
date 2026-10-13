import { useId } from 'react';
import {
  duplicateValues,
  enumValues,
  type EnumValue,
  type StructuredEditorProps,
} from './editor-models';
import { useEditorText } from './editor-text';
import { EditorHeader, EditorWarning, RemoveButton } from './editor-ui';
import './editors.css';

export default function EnumValuesEditor({ value, onChange, disabled }: StructuredEditorProps) {
  const t = useEditorText();
  const id = useId();
  const values = enumValues(value);
  const duplicates = duplicateValues(values.map((item) => item.value));
  const update = (index: number, patch: Partial<EnumValue>) =>
    onChange(values.map((item, position) => (position === index ? { ...item, ...patch } : item)));

  return (
    <fieldset className="structured-editor" disabled={disabled}>
      <legend className="sr-only">{t.enums}</legend>
      <EditorHeader
        title={t.enums}
        addLabel={t.addValue}
        disabled={disabled}
        onAdd={() => onChange([...values, { value: '', label: '', description: '' }])}
      />
      {!values.length && <p className="structured-editor-empty">{t.emptyEnums}</p>}
      <div className="structured-editor-rows">
        {values.map((item, index) => (
          <div className="structured-editor-row structured-enum-row" key={index}>
            <label className="structured-editor-field">
              <span>{t.value}</span>
              <input
                value={item.value}
                aria-label={`${t.value} ${index + 1}`}
                aria-invalid={duplicates.includes(item.value.trim())}
                aria-describedby={
                  duplicates.includes(item.value.trim()) ? `${id}-duplicates` : undefined
                }
                onChange={(event) => update(index, { value: event.target.value })}
              />
            </label>
            <label className="structured-editor-field">
              <span>{t.label}</span>
              <input
                value={item.label}
                aria-label={`${t.label} ${index + 1}`}
                onChange={(event) => update(index, { label: event.target.value })}
              />
            </label>
            <label className="structured-editor-field">
              <span>{t.description}</span>
              <input
                value={item.description}
                aria-label={`${t.description} ${index + 1}`}
                onChange={(event) => update(index, { description: event.target.value })}
              />
            </label>
            <RemoveButton
              label={`${t.removeValue} ${item.value || index + 1}`}
              disabled={disabled}
              onRemove={() => onChange(values.filter((_, position) => position !== index))}
            />
          </div>
        ))}
      </div>
      {!!duplicates.length && (
        <div id={`${id}-duplicates`}>
          <EditorWarning>
            {t.duplicateEnum}: {duplicates.join(', ')}
          </EditorWarning>
        </div>
      )}
    </fieldset>
  );
}
