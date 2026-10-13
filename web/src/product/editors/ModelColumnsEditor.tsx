import { useId } from 'react';
import {
  duplicateValues,
  modelColumns,
  type ModelColumn,
  type StructuredEditorProps,
} from './editor-models';
import { useEditorText } from './editor-text';
import { EditorHeader, EditorWarning, RemoveButton } from './editor-ui';
import './editors.css';

const sqlTypes = [
  'VARCHAR',
  'BOOLEAN',
  'INTEGER',
  'BIGINT',
  'DOUBLE',
  'DECIMAL(18,2)',
  'DATE',
  'TIMESTAMP',
  'JSON',
];

export default function ModelColumnsEditor({ value, onChange, disabled }: StructuredEditorProps) {
  const t = useEditorText();
  const id = useId();
  const columns = modelColumns(value);
  const duplicates = duplicateValues(
    columns.map((column) => column.name),
    true,
  );
  const update = (index: number, patch: Partial<ModelColumn>) =>
    onChange(
      columns.map((column, position) => (position === index ? { ...column, ...patch } : column)),
    );

  return (
    <fieldset className="structured-editor" disabled={disabled}>
      <legend className="sr-only">{t.columns}</legend>
      <EditorHeader
        title={t.columns}
        addLabel={t.addColumn}
        disabled={disabled}
        onAdd={() =>
          onChange([
            ...columns,
            { name: '', data_type: 'VARCHAR', nullable: true, description: '' },
          ])
        }
      />
      {!columns.length && <p className="structured-editor-empty">{t.emptyColumns}</p>}
      <datalist id={`${id}-types`}>
        {sqlTypes.map((type) => (
          <option key={type} value={type} />
        ))}
      </datalist>
      <div className="structured-editor-rows">
        {columns.map((column, index) => {
          const duplicate = duplicates.includes(column.name.trim().toLowerCase());
          return (
            <div className="structured-editor-row structured-column-row" key={index}>
              <label className="structured-editor-field">
                <span>{t.name}</span>
                <input
                  value={column.name}
                  aria-label={`${t.name} ${index + 1}`}
                  aria-invalid={duplicate}
                  aria-describedby={duplicate ? `${id}-duplicates` : undefined}
                  onChange={(event) => update(index, { name: event.target.value })}
                />
              </label>
              <label className="structured-editor-field">
                <span>{t.dataType}</span>
                <input
                  value={column.data_type}
                  list={`${id}-types`}
                  aria-label={`${t.dataType} ${index + 1}`}
                  onChange={(event) => update(index, { data_type: event.target.value })}
                />
              </label>
              <label className="structured-editor-field structured-column-description">
                <span>{t.description}</span>
                <input
                  value={column.description}
                  aria-label={`${t.description} ${index + 1}`}
                  onChange={(event) => update(index, { description: event.target.value })}
                />
              </label>
              <label className="structured-editor-check">
                <input
                  type="checkbox"
                  checked={!column.nullable}
                  aria-label={`${t.required} ${index + 1}`}
                  onChange={(event) => update(index, { nullable: !event.target.checked })}
                />
                <span>{t.required}</span>
              </label>
              <RemoveButton
                label={`${t.removeColumn} ${column.name || index + 1}`}
                disabled={disabled}
                onRemove={() => onChange(columns.filter((_, position) => position !== index))}
              />
            </div>
          );
        })}
      </div>
      {!!duplicates.length && (
        <div id={`${id}-duplicates`}>
          <EditorWarning>
            {t.duplicateColumn}: {duplicates.join(', ')}
          </EditorWarning>
        </div>
      )}
    </fieldset>
  );
}
