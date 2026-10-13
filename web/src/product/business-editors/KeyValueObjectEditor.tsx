import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import type { BusinessPropertyEditorProps } from './types';
import {
  credentialReference,
  credentialReferenceKey,
  duplicateIndexes,
  emptyValue,
  isRecord,
  sensitiveKey,
} from './business-models';
import { businessText } from './business-text';
import {
  BusinessEditorFrame,
  BusinessRemove,
  BusinessWarning,
  inputClass,
  InvalidBusinessValue,
} from './business-ui';

type ValueType = 'text' | 'number' | 'boolean' | 'null' | 'credential_ref';
interface ParameterRow {
  id: number;
  key: string;
  type: ValueType;
  raw: string | boolean;
  hiddenSecret: boolean;
}
let rowNumber = 0;

function objectRows(value: unknown): ParameterRow[] | null {
  if (emptyValue(value)) return [];
  if (
    !isRecord(value) ||
    !Object.values(value).every(
      (item) => item === null || ['string', 'number', 'boolean'].includes(typeof item),
    )
  )
    return null;
  return Object.entries(value).map(([key, item]) => {
    const reference = credentialReferenceKey(key);
    const sensitive = sensitiveKey(key) && !reference;
    const hiddenSecret = sensitive || (reference && !credentialReference(item) && item !== '');
    return {
      id: rowNumber++,
      key,
      type:
        reference || sensitive
          ? 'credential_ref'
          : item === null
            ? 'null'
            : typeof item === 'number'
              ? 'number'
              : typeof item === 'boolean'
                ? 'boolean'
                : 'text',
      raw: hiddenSecret ? '' : typeof item === 'boolean' ? item : String(item),
      hiddenSecret,
    };
  });
}

export default function KeyValueObjectEditor({
  kind,
  value,
  onChange,
  disabled,
}: BusinessPropertyEditorProps) {
  const { i18n } = useTranslation();
  const text = businessText(i18n.language);
  const runtimeSensitive = i18n.language.startsWith('zh')
    ? '运行变量只支持非敏感参数。请移除该项；当前执行器不消费凭据引用。'
    : 'Runtime variables support non-secret parameters only. Remove this entry; the current executor does not consume credential references.';
  const [rows, setRows] = useState(() => objectRows(value));
  const emitted = useRef(value);
  useEffect(() => {
    if (value !== emitted.current) {
      emitted.current = value;
      setRows(objectRows(value));
    }
  }, [value]);
  const duplicates = duplicateIndexes(rows?.map((row) => row.key) ?? []);
  const issueFor = (row: ParameterRow, index: number) => {
    if (kind === 'development_config' && row.type === 'credential_ref') return runtimeSensitive;
    if (!row.key.trim()) return text.missing;
    if (duplicates.has(index)) return text.duplicate;
    if (sensitiveKey(row.key) && !credentialReferenceKey(row.key)) return text.sensitiveBlocked;
    if (row.type === 'credential_ref')
      return !credentialReferenceKey(row.key)
        ? text.referenceKey
        : credentialReference(row.raw)
          ? ''
          : text.invalidReference;
    if (
      row.type === 'number' &&
      (typeof row.raw !== 'string' || !row.raw.trim() || !Number.isFinite(Number(row.raw)))
    )
      return text.invalidNumber;
    return '';
  };

  const commitRows = (next: ParameterRow[]) => {
    setRows(next);
    const duplicateKeys = duplicateIndexes(next.map((row) => row.key));
    if (
      next.some(
        (row, index) =>
          duplicateKeys.has(index) ||
          !row.key.trim() ||
          row.hiddenSecret ||
          (kind === 'development_config' && row.type === 'credential_ref') ||
          (sensitiveKey(row.key) && !credentialReferenceKey(row.key)) ||
          (row.type === 'credential_ref' &&
            (!credentialReferenceKey(row.key) || !credentialReference(row.raw))) ||
          (row.type === 'number' &&
            (typeof row.raw !== 'string' || !row.raw.trim() || !Number.isFinite(Number(row.raw)))),
      )
    )
      return;
    const object = Object.fromEntries(
      next.map((row) => [
        row.key.trim(),
        row.type === 'null'
          ? null
          : row.type === 'number'
            ? Number(row.raw)
            : row.type === 'boolean'
              ? row.raw === true
              : typeof row.raw === 'string'
                ? row.raw
                : String(row.raw),
      ]),
    );
    emitted.current = object;
    onChange(object);
  };
  const update = (index: number, patch: Partial<ParameterRow>) => {
    if (rows)
      commitRows(rows.map((row, current) => (current === index ? { ...row, ...patch } : row)));
  };
  if (!rows)
    return (
      <InvalidBusinessValue
        text={text.incompatible}
        resetLabel={text.reset}
        disabled={disabled}
        onReset={() => {
          emitted.current = {};
          setRows([]);
          onChange(emitted.current);
        }}
      />
    );

  return (
    <BusinessEditorFrame
      title={text.keyValues}
      addLabel={text.add}
      disabled={disabled}
      onAdd={() =>
        commitRows([
          ...rows,
          { id: rowNumber++, key: '', type: 'text', raw: '', hiddenSecret: false },
        ])
      }
    >
      {rows.map((row, index) => {
        const issue = issueFor(row, index);
        const sensitive = sensitiveKey(row.key) && !credentialReferenceKey(row.key);
        return (
          <div
            key={row.id}
            className="space-y-3 rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-3"
          >
            <div className="grid items-start gap-3 sm:grid-cols-[1fr_1fr_auto]">
              <label className="text-xs text-[var(--color-text-secondary)]">
                {text.key}
                <input
                  className={inputClass + ' mt-1.5'}
                  aria-label={`${text.key} ${index + 1}`}
                  value={row.key}
                  required
                  disabled={disabled}
                  aria-invalid={!!issue}
                  ref={(node) => node?.setCustomValidity(issue)}
                  onChange={(event) => {
                    const key = event.target.value;
                    const credential = credentialReferenceKey(key) || sensitiveKey(key);
                    update(index, {
                      key,
                      ...(credential && row.type !== 'credential_ref'
                        ? { type: 'credential_ref', raw: '', hiddenSecret: false }
                        : !credential && row.type === 'credential_ref'
                          ? { type: 'text', raw: '', hiddenSecret: false }
                          : {}),
                    });
                  }}
                />
              </label>
              <label className="text-xs text-[var(--color-text-secondary)]">
                {text.valueType}
                <select
                  className={inputClass + ' mt-1.5'}
                  aria-label={`${text.valueType} ${index + 1}`}
                  value={row.type}
                  disabled={disabled || sensitive || credentialReferenceKey(row.key)}
                  onChange={(event) => {
                    const type = event.target.value as ValueType;
                    update(index, {
                      type,
                      raw: type === 'boolean' ? false : '',
                      hiddenSecret: false,
                      ...(type === 'credential_ref' && row.key && !credentialReferenceKey(row.key)
                        ? { key: row.key + '_credential_ref' }
                        : {}),
                    });
                  }}
                >
                  <option value="text">{text.text}</option>
                  <option value="number">{text.number}</option>
                  <option value="boolean">{text.boolean}</option>
                  {kind === 'development_config' && <option value="null">NULL</option>}
                  {kind !== 'development_config' && (
                    <option value="credential_ref">{text.credentialType}</option>
                  )}
                </select>
              </label>
              <div className="sm:pt-5">
                <BusinessRemove
                  label={`${text.remove} ${text.key} ${index + 1}`}
                  disabled={disabled}
                  onRemove={() => commitRows(rows.filter((_, current) => current !== index))}
                />
              </div>
            </div>
            {row.hiddenSecret && <BusinessWarning>{text.secretHidden}</BusinessWarning>}
            {sensitive ? (
              <div className="space-y-2">
                <BusinessWarning>
                  {kind === 'development_config' ? runtimeSensitive : text.sensitiveBlocked}
                </BusinessWarning>
                {kind !== 'development_config' && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={disabled}
                    onClick={() =>
                      update(index, {
                        key: row.key + '_credential_ref',
                        type: 'credential_ref',
                        raw: '',
                        hiddenSecret: false,
                      })
                    }
                  >
                    {text.migrate}
                  </Button>
                )}
              </div>
            ) : (
              <label className="block text-xs text-[var(--color-text-secondary)]">
                {row.type === 'credential_ref' ? text.credentialId : text.value}
                {row.type === 'null' ? (
                  <span className="block mt-1.5">NULL</span>
                ) : row.type === 'boolean' ? (
                  <select
                    className={inputClass + ' mt-1.5'}
                    aria-label={`${text.value} ${index + 1}`}
                    value={row.raw === true ? 'true' : 'false'}
                    disabled={disabled}
                    onChange={(event) => update(index, { raw: event.target.value === 'true' })}
                  >
                    <option value="true">{text.true}</option>
                    <option value="false">{text.false}</option>
                  </select>
                ) : (
                  <input
                    className={inputClass + ' mt-1.5'}
                    aria-label={`${row.type === 'credential_ref' ? text.credentialId : text.value} ${index + 1}`}
                    value={typeof row.raw === 'string' ? row.raw : ''}
                    disabled={disabled}
                    autoComplete="off"
                    inputMode={row.type === 'number' ? 'decimal' : 'text'}
                    placeholder={row.type === 'credential_ref' ? 'credential:<UUID>' : undefined}
                    required={row.type === 'credential_ref' || row.type === 'number'}
                    aria-invalid={!!issue}
                    ref={(node) => node?.setCustomValidity(issue)}
                    onChange={(event) =>
                      update(index, { raw: event.target.value, hiddenSecret: false })
                    }
                  />
                )}
              </label>
            )}
            {row.type === 'credential_ref' && !sensitive && (
              <p className="text-xs leading-relaxed text-[var(--color-text-tertiary)]">
                {text.credentialHint}
              </p>
            )}
            {issue && !sensitive && <BusinessWarning>{issue}</BusinessWarning>}
          </div>
        );
      })}
      {!rows.length && (
        <p className="py-3 text-center text-xs text-[var(--color-text-tertiary)]">
          {text.objectEmpty}
        </p>
      )}
    </BusinessEditorFrame>
  );
}
