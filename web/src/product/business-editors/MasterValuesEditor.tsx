import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { ErrorBanner } from '../../components/ui';
import { useAuthStore } from '../../stores/auth';
import type { ProductResource } from '../resource-types';
import { apiErrorMessage } from '../../lib/apiError';
import type { BusinessPropertyEditorProps } from './types';
import { isRecord } from './business-models';
import { inputClass, InvalidBusinessValue } from './business-ui';
import JsonValueInput from './JsonValueInput';

export default function MasterValuesEditor({
  value,
  onChange,
  properties,
  disabled,
}: BusinessPropertyEditorProps) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  const modelId = typeof properties?.model_id === 'string' ? properties.model_id : '';
  const model = useQuery({
    queryKey: ['master-value-model', user?.tenant_id, user?.id, modelId],
    queryFn: async ({ signal }) => {
      const result = await client.get<ProductResource>(
        `/product/resources/master_data_model/${encodeURIComponent(modelId)}`,
        { signal, headers: { Authorization: `Bearer ${token}` } },
      );
      if (
        result.data.tenant_id !== user?.tenant_id ||
        result.data.kind !== 'master_data_model' ||
        result.data.id !== modelId
      )
        throw new Error(text('模型身份不匹配', 'Model identity mismatch'));
      return result.data;
    },
    enabled: !!user && !!token && !!modelId,
  });
  if (!modelId)
    return (
      <p className="text-sm text-[var(--color-text-tertiary)]">
        {text('先选择已发布的主数据模型', 'Select a published master data model first')}
      </p>
    );
  if (model.error)
    return (
      <ErrorBanner
        message={apiErrorMessage(model.error, text('模型加载失败', 'Could not load the model'))}
      />
    );
  if (!model.data)
    return <p className="text-sm">{text('正在读取模型结构', 'Loading model schema')}</p>;
  if (model.data.status !== 'published')
    return (
      <ErrorBanner
        message={text('模型须先发布才能维护主数据', 'Publish the model before maintaining records')}
      />
    );
  const values = value === undefined || value === '' ? {} : isRecord(value) ? value : null;
  if (!values)
    return (
      <InvalidBusinessValue
        text={text('字段值必须是模型字段组成的对象', 'Values must be a model field object')}
        resetLabel={text('重建字段表单', 'Reset fields')}
        onReset={() => onChange({})}
        disabled={disabled}
      />
    );
  const columns = Array.isArray(model.data.properties.columns)
    ? model.data.properties.columns.filter(isRecord)
    : [];
  const extra = Object.keys(values).filter((key) => !columns.some((column) => column.name === key));
  const update = (key: string, next: unknown) => onChange({ ...values, [key]: next });
  return (
    <div className="space-y-4">
      {!!extra.length && (
        <ErrorBanner
          message={`${text('存在未定义字段，需要核对', 'Undefined fields require review')}: ${extra.join(', ')}`}
        />
      )}
      {columns.map((column) => {
        if (typeof column.name !== 'string' || typeof column.data_type !== 'string') return null;
        const name = column.name;
        const dataType = column.data_type.toUpperCase();
        const nullable = column.nullable === true;
        const current = values[name];
        const numeric = [
          'TINYINT',
          'SMALLINT',
          'INTEGER',
          'INT',
          'BIGINT',
          'FLOAT',
          'DOUBLE',
        ].includes(dataType);
        return (
          <div key={name} className="rounded-md border border-[var(--color-border-default)] p-3">
            <div className="mb-2 flex items-center justify-between gap-2 text-sm">
              <label htmlFor={`master-value-${name}`}>
                {name} · {dataType}
                {!nullable && ' *'}
              </label>
              {nullable && (
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={current === null}
                    disabled={disabled}
                    onChange={(event) => update(name, event.target.checked ? null : '')}
                  />
                  NULL
                </label>
              )}
            </div>
            {current !== null &&
              (dataType === 'BOOLEAN' || dataType === 'BOOL' ? (
                <select
                  id={`master-value-${name}`}
                  className={inputClass}
                  required={!nullable}
                  disabled={disabled}
                  value={current === true ? 'true' : current === false ? 'false' : ''}
                  onChange={(event) => update(name, event.target.value === 'true')}
                >
                  <option value="">{text('选择布尔值', 'Select boolean value')}</option>
                  <option value="true">true</option>
                  <option value="false">false</option>
                </select>
              ) : dataType === 'JSON' ? (
                <JsonValueInput
                  key={`${modelId}:${name}`}
                  id={`master-value-${name}`}
                  value={current}
                  onChange={(next) => update(name, next)}
                  required={!nullable}
                  disabled={disabled}
                />
              ) : (
                <input
                  id={`master-value-${name}`}
                  className={inputClass}
                  type={numeric ? 'number' : dataType === 'DATE' ? 'date' : 'text'}
                  step={numeric ? 'any' : undefined}
                  required={!nullable}
                  disabled={disabled}
                  value={
                    current === undefined
                      ? ''
                      : typeof current === 'object'
                        ? JSON.stringify(current)
                        : String(current)
                  }
                  onChange={(event) => {
                    const next = event.target.value;
                    const parsed = numeric && next !== '' ? Number(next) : next;
                    const integer = numeric && !['FLOAT', 'DOUBLE'].includes(dataType);
                    event.target.setCustomValidity(
                      integer && typeof parsed === 'number' && !Number.isSafeInteger(parsed)
                        ? text('请输入可精确表示的整数', 'Enter an exactly representable integer')
                        : '',
                    );
                    update(name, parsed);
                  }}
                />
              ))}
            {typeof column.description === 'string' && column.description && (
              <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">{column.description}</p>
            )}
          </div>
        );
      })}
    </div>
  );
}
