import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { FormInput } from '../components/ui';
import type { ResourceField } from './catalog-types';
import ReferencePicker from './ReferencePicker';
import ExpiryInput from './ExpiryInput';
import WorkflowScheduleField from './WorkflowScheduleField';
import MetadataFieldPicker from './MetadataFieldPicker';
import DerivedMetricFieldPicker from './DerivedMetricFieldPicker';
import RecognitionSourceTypes from './RecognitionSourceTypes';
import StructuredPropertyEditor, {
  hasStructuredPropertyEditor,
} from './editors/StructuredPropertyEditor';
import { localized } from './ui-text';
import BusinessPropertyEditor, { hasBusinessPropertyEditor } from './BusinessPropertyEditor';
import DesignPropertyEditor, {
  hasDesignPropertyEditor,
} from './design-editors/DesignPropertyEditor';

interface Props {
  kind: string;
  field: ResourceField;
  value: unknown;
  onChange: (value: unknown) => void;
  disabled?: boolean;
  properties?: Record<string, unknown>;
}

export default function ResourceFieldInput({
  kind,
  field,
  value,
  onChange,
  disabled,
  properties,
}: Props) {
  const { i18n } = useTranslation();
  const id = useId();
  if (
    kind === 'development_config' &&
    field.key === 'source_kind' &&
    properties?.scope === 'source'
  )
    field = { ...field, required: true };
  const label = localized(field.label, i18n.language);
  if (
    kind === 'development_workflow' &&
    ['cron_expression', 'timezone', 'retry_limit'].includes(field.key)
  )
    return (
      <WorkflowScheduleField field={field} value={value} onChange={onChange} disabled={disabled} />
    );
  if (
    kind === 'development_config' &&
    ['source_kind', 'source_id'].includes(field.key) &&
    properties?.scope !== 'source'
  )
    return null;
  if (kind === 'development_config' && field.key === 'source_id') {
    const sourceKind = properties?.source_kind;
    if (sourceKind !== 'sql_document' && sourceKind !== 'development_workflow')
      return (
        <p className="text-sm text-[var(--color-text-tertiary)]">
          {i18n.language.startsWith('zh')
            ? '请先选择实际开发来源类型'
            : 'Select a development source kind first'}
        </p>
      );
    return (
      <ReferencePicker
        kind={sourceKind}
        label={label}
        value={typeof value === 'string' ? value : ''}
        onChange={onChange}
        disabled={disabled}
        required
      />
    );
  }
  if (kind === 'recognition_rule' && field.key === 'source_types')
    return (
      <RecognitionSourceTypes label={label} value={value} onChange={onChange} disabled={disabled} />
    );
  if (kind === 'derived_metric' && field.key === 'dimensions')
    return (
      <DerivedMetricFieldPicker
        atomicId={
          typeof properties?.atomic_metric_id === 'string' ? properties.atomic_metric_id : ''
        }
        label={label}
        value={value}
        onChange={onChange}
        disabled={disabled}
      />
    );
  if (
    (kind === 'atomic_metric' &&
      ['grain_columns', 'dimensions', 'time_column'].includes(field.key)) ||
    (kind === 'metric_modifier' && field.key === 'dimension')
  ) {
    return (
      <MetadataFieldPicker
        metadataId={
          typeof properties?.metadata_object_id === 'string' ? properties.metadata_object_id : ''
        }
        label={label}
        value={value}
        onChange={onChange}
        required={field.required}
        disabled={disabled}
        multiple={['grain_columns', 'dimensions'].includes(field.key)}
        timeOnly={field.key === 'time_column'}
      />
    );
  }
  if (kind === 'asset_entry' && field.key === 'object_id') {
    const sourceKind = typeof properties?.object_kind === 'string' ? properties.object_kind : '';
    if (!sourceKind)
      return (
        <p className="text-sm text-[var(--color-text-tertiary)]">
          {i18n.language.startsWith('zh')
            ? '请先选择资产类型，再选择真实来源'
            : 'Choose an asset type before selecting its source'}
        </p>
      );
    return (
      <ReferencePicker
        key={sourceKind}
        kind={sourceKind}
        label={label}
        value={typeof value === 'string' ? value : ''}
        onChange={onChange}
        required={field.required}
        disabled={disabled}
      />
    );
  }
  if (
    (['data_access_request', 'service_subscription'].includes(kind) &&
      field.key === 'expires_at') ||
    (kind === 'service_application' && field.key === 'credential_expires_at')
  )
    return (
      <ExpiryInput
        label={label}
        value={value}
        onChange={onChange}
        disabled={disabled}
        description={
          i18n.language.startsWith('zh')
            ? '留空时申请 30 天访问权限；最长 365 天'
            : 'Leave empty to request 30 days of access; maximum 365 days'
        }
      />
    );

  if (field.referenceKind) {
    return (
      <ReferencePicker
        kind={field.referenceKind}
        resourceKind={kind}
        label={label}
        value={typeof value === 'string' ? value : ''}
        onChange={onChange}
        required={field.required}
        disabled={disabled}
      />
    );
  }
  if (field.type === 'boolean') {
    return (
      <label className="flex items-center gap-2 py-2 text-sm">
        <input
          type="checkbox"
          checked={value === true}
          onChange={(event) => onChange(event.target.checked)}
          disabled={disabled}
        />
        {label}
      </label>
    );
  }
  if (field.type === 'select') {
    return (
      <div>
        <label
          htmlFor={id}
          className="mb-1.5 block text-sm font-medium text-[var(--color-text-secondary)]"
        >
          {label}
        </label>
        <select
          id={id}
          value={typeof value === 'string' ? value : ''}
          onChange={(event) => onChange(event.target.value)}
          required={field.required}
          disabled={disabled}
          className="w-full rounded-md border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm"
        >
          <option value="">—</option>
          {field.options?.map((option) => (
            <option key={option.value} value={option.value}>
              {localized(option.label, i18n.language)}
            </option>
          ))}
        </select>
      </div>
    );
  }
  if (field.type === 'json') {
    const isCheckpoint = kind === 'realtime_task' && field.key === 'checkpoint';
    return (
      <div className="space-y-2">
        <div className="text-sm font-medium text-[var(--color-text-secondary)]">{label}</div>
        {hasStructuredPropertyEditor(kind, field.key) ? (
          <StructuredPropertyEditor
            kind={kind}
            fieldKey={field.key}
            value={value}
            onChange={onChange}
            disabled={disabled}
          />
        ) : hasDesignPropertyEditor(kind, field.key) ? (
          <DesignPropertyEditor
            kind={kind}
            fieldKey={field.key}
            value={value}
            onChange={onChange}
            disabled={disabled}
            properties={properties}
          />
        ) : hasBusinessPropertyEditor(kind, field.key) ? (
          <BusinessPropertyEditor
            kind={kind}
            fieldKey={field.key}
            value={value}
            onChange={onChange}
            disabled={disabled}
            properties={properties}
          />
        ) : (
          <details className="rounded-md border border-[var(--color-border-default)] p-3">
            <summary className="text-sm text-[var(--color-text-secondary)]">
              {i18n.language.startsWith('zh')
                ? isCheckpoint
                  ? '运行位点（只读）'
                  : '高级配置'
                : isCheckpoint
                  ? 'Checkpoint (read only)'
                  : 'Advanced configuration'}
            </summary>
            <div className="mt-3">
              <FormInput
                label={`${label} · JSON`}
                type="textarea"
                value={
                  typeof value === 'string'
                    ? value
                    : value === undefined
                      ? ''
                      : JSON.stringify(value, null, 2)
                }
                onChange={onChange}
                rows={5}
                disabled={disabled || isCheckpoint}
              />
            </div>
          </details>
        )}
      </div>
    );
  }
  return (
    <FormInput
      label={label}
      type={field.type === 'textarea' ? 'textarea' : field.type === 'number' ? 'number' : 'text'}
      value={typeof value === 'string' || typeof value === 'number' ? value : ''}
      onChange={onChange}
      required={field.required}
      disabled={disabled}
    />
  );
}
