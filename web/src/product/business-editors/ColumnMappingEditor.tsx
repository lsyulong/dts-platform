import { useTranslation } from 'react-i18next';
import type { BusinessPropertyEditorProps, ColumnMapping } from './types';
import { duplicateIndexes, emptyValue, isRecord } from './business-models';
import { businessText } from './business-text';
import {
  BusinessEditorFrame,
  BusinessRemove,
  BusinessWarning,
  inputClass,
  InvalidBusinessValue,
} from './business-ui';

function mappingRows(value: unknown): ColumnMapping[] | null {
  if (emptyValue(value)) return [];
  if (
    !Array.isArray(value) ||
    !value.every(
      (row) =>
        isRecord(row) &&
        typeof row.source_column === 'string' &&
        typeof row.target_column === 'string' &&
        Object.keys(row).every((key) => ['source_column', 'target_column'].includes(key)),
    )
  )
    return null;
  return value;
}

export default function ColumnMappingEditor({
  value,
  onChange,
  disabled,
}: BusinessPropertyEditorProps) {
  const { i18n } = useTranslation();
  const text = businessText(i18n.language);
  const rows = mappingRows(value);
  if (!rows)
    return (
      <InvalidBusinessValue
        text={text.incompatible}
        resetLabel={text.reset}
        onReset={() => onChange([])}
        disabled={disabled}
      />
    );
  const duplicates = duplicateIndexes(rows.map((row) => row.target_column));
  const change = (index: number, key: keyof ColumnMapping, next: string) =>
    onChange(rows.map((row, current) => (current === index ? { ...row, [key]: next } : row)));
  return (
    <BusinessEditorFrame
      title={text.mapping}
      addLabel={text.add}
      onAdd={() => onChange([...rows, { source_column: '', target_column: '' }])}
      disabled={disabled}
    >
      {rows.map((row, index) => (
        <div
          key={index}
          className="grid items-start gap-2 rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-3 sm:grid-cols-[1fr_1fr_auto]"
        >
          <label className="min-w-0 text-xs text-[var(--color-text-secondary)]">
            {text.sourceColumn}
            <input
              className={inputClass + ' mt-1.5'}
              aria-label={`${text.sourceColumn} ${index + 1}`}
              value={row.source_column}
              required
              disabled={disabled}
              ref={(node) => node?.setCustomValidity(row.source_column.trim() ? '' : text.missing)}
              onChange={(event) => change(index, 'source_column', event.target.value)}
            />
          </label>
          <label className="min-w-0 text-xs text-[var(--color-text-secondary)]">
            {text.targetColumn}
            <input
              className={inputClass + ' mt-1.5'}
              aria-label={`${text.targetColumn} ${index + 1}`}
              value={row.target_column}
              required
              disabled={disabled}
              aria-invalid={duplicates.has(index)}
              ref={(node) =>
                node?.setCustomValidity(
                  duplicates.has(index)
                    ? text.duplicateTarget
                    : row.target_column.trim()
                      ? ''
                      : text.missing,
                )
              }
              onChange={(event) => change(index, 'target_column', event.target.value)}
            />
          </label>
          <div className="sm:pt-5">
            <BusinessRemove
              label={`${text.remove} ${text.mapping} ${index + 1}`}
              disabled={disabled}
              onRemove={() => onChange(rows.filter((_, current) => current !== index))}
            />
          </div>
        </div>
      ))}
      {duplicates.size > 0 && <BusinessWarning>{text.duplicateTarget}</BusinessWarning>}
      {!rows.length && (
        <p className="py-3 text-center text-xs text-[var(--color-text-tertiary)]">{text.empty}</p>
      )}
    </BusinessEditorFrame>
  );
}
