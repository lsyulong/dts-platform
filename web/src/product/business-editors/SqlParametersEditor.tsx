import { useTranslation } from 'react-i18next';
import type { BusinessPropertyEditorProps, SqlParameter } from './types';
import { duplicateIndexes, emptyValue, isRecord } from './business-models';
import { businessText } from './business-text';
import { isInt64Parameter } from '../int64-parameter';
import {
  BusinessEditorFrame,
  BusinessRemove,
  BusinessWarning,
  inputClass,
  InvalidBusinessValue,
} from './business-ui';

const sqlTypes = ['VARCHAR', 'BIGINT', 'DOUBLE', 'BOOLEAN', 'DATE', 'TIMESTAMP'];
const integerType = (type: string) =>
  /^(TINYINT|SMALLINT|INTEGER|BIGINT|UTINYINT|USMALLINT|UINTEGER|UBIGINT)$/.test(
    type.toUpperCase(),
  );
const numericType = (type: string) =>
  integerType(type) || ['DOUBLE', 'FLOAT', 'REAL', 'DECIMAL'].includes(type.toUpperCase());

function parameterRows(value: unknown): SqlParameter[] | null {
  if (emptyValue(value)) return [];
  if (
    !Array.isArray(value) ||
    !value.every(
      (row) =>
        isRecord(row) &&
        typeof row.name === 'string' &&
        typeof row.data_type === 'string' &&
        typeof row.required === 'boolean' &&
        Object.keys(row).every((key) =>
          ['name', 'data_type', 'required', 'default_value'].includes(key),
        ) &&
        (row.default_value === undefined ||
          row.default_value === null ||
          ['string', 'number', 'boolean'].includes(typeof row.default_value)),
    )
  )
    return null;
  return value;
}

function defaultIssue(row: SqlParameter) {
  if (row.default_value === undefined || row.default_value === null) return false;
  if (row.data_type.toUpperCase() === 'BOOLEAN') return typeof row.default_value !== 'boolean';
  if (row.data_type.toUpperCase() === 'BIGINT') return !isInt64Parameter(row.default_value);
  if (numericType(row.data_type))
    return (
      typeof row.default_value !== 'number' ||
      !Number.isFinite(row.default_value) ||
      (integerType(row.data_type) && !Number.isSafeInteger(row.default_value))
    );
  return typeof row.default_value !== 'string';
}

function startingDefault(type: string) {
  return type.toUpperCase() === 'BOOLEAN' ? false : numericType(type) ? 0 : '';
}

export default function SqlParametersEditor({
  value,
  onChange,
  disabled,
}: BusinessPropertyEditorProps) {
  const { i18n } = useTranslation();
  const text = businessText(i18n.language);
  const rows = parameterRows(value);
  if (!rows)
    return (
      <InvalidBusinessValue
        text={text.incompatible}
        resetLabel={text.reset}
        onReset={() => onChange([])}
        disabled={disabled}
      />
    );
  const duplicates = duplicateIndexes(rows.map((row) => row.name));
  const change = (index: number, next: SqlParameter) =>
    onChange(rows.map((row, current) => (current === index ? next : row)));
  return (
    <BusinessEditorFrame
      title={text.parameterRows}
      addLabel={text.add}
      onAdd={() => onChange([...rows, { name: '', data_type: 'VARCHAR', required: false }])}
      disabled={disabled}
    >
      {rows.map((row, index) => {
        const mode =
          row.default_value === undefined ? 'none' : row.default_value === null ? 'null' : 'value';
        const issue = defaultIssue(row);
        const unknownType = !sqlTypes.includes(row.data_type);
        return (
          <div
            key={index}
            className="space-y-3 rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-3"
          >
            <div className="grid items-start gap-3 sm:grid-cols-[1fr_1fr_auto_auto]">
              <label className="min-w-0 text-xs text-[var(--color-text-secondary)]">
                {text.parameterName}
                <input
                  className={inputClass + ' mt-1.5'}
                  aria-label={`${text.parameterName} ${index + 1}`}
                  value={row.name}
                  required
                  disabled={disabled}
                  aria-invalid={duplicates.has(index)}
                  ref={(node) =>
                    node?.setCustomValidity(
                      duplicates.has(index) ? text.duplicate : row.name.trim() ? '' : text.missing,
                    )
                  }
                  onChange={(event) => change(index, { ...row, name: event.target.value })}
                />
              </label>
              <label className="min-w-0 text-xs text-[var(--color-text-secondary)]">
                {text.dataType}
                <select
                  className={inputClass + ' mt-1.5'}
                  aria-label={`${text.dataType} ${index + 1}`}
                  value={row.data_type}
                  disabled={disabled}
                  required
                  onChange={(event) =>
                    change(index, {
                      ...row,
                      data_type: event.target.value,
                      ...(mode === 'value'
                        ? { default_value: startingDefault(event.target.value) }
                        : {}),
                    })
                  }
                >
                  {unknownType && <option value={row.data_type}>{row.data_type}</option>}
                  {sqlTypes.map((type) => (
                    <option key={type} value={type}>
                      {type}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)] sm:pt-8">
                <input
                  type="checkbox"
                  checked={row.required}
                  disabled={disabled}
                  aria-label={`${text.required} ${index + 1}`}
                  onChange={(event) => change(index, { ...row, required: event.target.checked })}
                />
                {text.required}
              </label>
              <div className="sm:pt-5">
                <BusinessRemove
                  label={`${text.remove} ${text.parameterName} ${index + 1}`}
                  disabled={disabled}
                  onRemove={() => onChange(rows.filter((_, current) => current !== index))}
                />
              </div>
            </div>
            {unknownType && (
              <p className="text-xs text-[var(--color-text-tertiary)]">{text.unknownType}</p>
            )}
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="text-xs text-[var(--color-text-secondary)]">
                {text.defaultMode}
                <select
                  className={inputClass + ' mt-1.5'}
                  aria-label={`${text.defaultMode} ${index + 1}`}
                  value={mode}
                  disabled={disabled}
                  onChange={(event) => {
                    const next = { ...row };
                    if (event.target.value === 'none') delete next.default_value;
                    else
                      next.default_value =
                        event.target.value === 'null' ? null : startingDefault(row.data_type);
                    change(index, next);
                  }}
                >
                  <option value="none">{text.noDefault}</option>
                  <option value="null">{text.nullDefault}</option>
                  <option value="value">{text.scalarDefault}</option>
                </select>
              </label>
              {mode === 'value' && (
                <label className="text-xs text-[var(--color-text-secondary)]">
                  {text.defaultValue}
                  {row.data_type.toUpperCase() === 'BOOLEAN' ? (
                    <select
                      className={inputClass + ' mt-1.5'}
                      aria-label={`${text.defaultValue} ${index + 1}`}
                      value={
                        row.default_value === true
                          ? 'true'
                          : row.default_value === false
                            ? 'false'
                            : ''
                      }
                      disabled={disabled}
                      required
                      ref={(node) => node?.setCustomValidity(issue ? text.invalidDefault : '')}
                      onChange={(event) =>
                        change(index, { ...row, default_value: event.target.value === 'true' })
                      }
                    >
                      {issue && <option value="">—</option>}
                      <option value="true">{text.true}</option>
                      <option value="false">{text.false}</option>
                    </select>
                  ) : (
                    <input
                      className={inputClass + ' mt-1.5'}
                      aria-label={`${text.defaultValue} ${index + 1}`}
                      inputMode={numericType(row.data_type) ? 'decimal' : 'text'}
                      value={String(row.default_value ?? '')}
                      disabled={disabled}
                      aria-invalid={issue}
                      ref={(node) => node?.setCustomValidity(issue ? text.invalidDefault : '')}
                      onChange={(event) => {
                        const next = event.target.value;
                        const parsed = Number(next);
                        const validNumber =
                          next.trim() !== '' &&
                          Number.isFinite(parsed) &&
                          (!integerType(row.data_type) ||
                            (/^[+-]?\d+$/.test(next) && Number.isSafeInteger(parsed)));
                        change(index, {
                          ...row,
                          default_value:
                            row.data_type.toUpperCase() === 'BIGINT'
                              ? next
                              : numericType(row.data_type) && validNumber
                                ? parsed
                                : next,
                        });
                      }}
                    />
                  )}
                </label>
              )}
            </div>
            {issue && <BusinessWarning>{text.invalidDefault}</BusinessWarning>}
          </div>
        );
      })}
      {duplicates.size > 0 && <BusinessWarning>{text.duplicate}</BusinessWarning>}
      {!rows.length && (
        <p className="py-3 text-center text-xs text-[var(--color-text-tertiary)]">{text.empty}</p>
      )}
    </BusinessEditorFrame>
  );
}
