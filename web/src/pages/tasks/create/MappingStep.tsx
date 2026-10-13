import { useTranslation } from 'react-i18next';
import { Plus, Trash2 } from 'lucide-react';
import { FormInput } from '../../../components/ui';
import type { DatasourceTable } from '../../../hooks/useApi';
import {
  tableMappingsSchema,
  createEmptyTableMapping,
  type TableMapping,
} from '../../../lib/validations';

type Props = {
  mappings: TableMapping[];
  batchSize: string;
  availableTables: DatasourceTable[];
  loadingTables: boolean;
  onChange: (mappings: TableMapping[]) => void;
  onBatchSizeChange: (value: string) => void;
};

export function MappingStep({
  mappings,
  batchSize,
  availableTables,
  loadingTables,
  onChange,
  onBatchSizeChange,
}: Props) {
  const { t } = useTranslation();
  const updateMapping = (index: number, field: keyof TableMapping, value: string) => {
    onChange(
      mappings.map((mapping, i) => (i === index ? { ...mapping, [field]: value } : mapping)),
    );
  };
  const duplicateTarget = tableMappingsSchema
    .safeParse(mappings)
    .error?.issues.some((issue) => issue.message === 'duplicate_target');
  const addDiscoveredTable = (identity: string) => {
    const table = availableTables.find(({ schema, name }) => `${schema}.${name}` === identity);
    if (!table) return;
    if (
      mappings.some(
        (mapping) => mapping.source_schema === table.schema && mapping.source_table === table.name,
      )
    ) {
      return;
    }
    const discovered: TableMapping = {
      source_schema: table.schema,
      source_table: table.name,
      target_schema: '',
      target_table: table.name,
    };
    const emptyIndex = mappings.findIndex(
      (mapping) => !mapping.source_table && !mapping.target_table,
    );
    if (emptyIndex >= 0) {
      onChange(mappings.map((mapping, index) => (index === emptyIndex ? discovered : mapping)));
    } else if (mappings.length < 100) {
      onChange([...mappings, discovered]);
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <h3 className="text-sm font-medium text-[var(--color-text-secondary)]">
          {t('tasks.mappingConfig')}
        </h3>
        <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">{t('tasks.mappingHint')}</p>
      </div>
      <label className="block text-sm text-[var(--color-text-secondary)]">
        <span className="mb-1.5 block">{t('tasks.mapping.discoveredTable')}</span>
        <select
          defaultValue=""
          disabled={loadingTables || availableTables.length === 0 || mappings.length >= 100}
          onChange={(event) => {
            addDiscoveredTable(event.target.value);
            event.target.value = '';
          }}
          className="w-full rounded-md border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm text-[var(--color-text-primary)] disabled:opacity-60"
        >
          <option value="">
            {loadingTables
              ? t('tasks.mapping.discovering')
              : availableTables.length === 0
                ? t('tasks.mapping.noDiscoveredTables')
                : t('tasks.mapping.selectDiscoveredTable')}
          </option>
          {availableTables.map((table) => (
            <option key={`${table.schema}.${table.name}`} value={`${table.schema}.${table.name}`}>
              {table.schema}.{table.name}
            </option>
          ))}
        </select>
      </label>
      {mappings.map((mapping, index) => (
        <fieldset
          key={index}
          className="space-y-3 rounded-lg border border-[var(--color-border-default)] p-4"
        >
          <legend className="px-1 text-sm text-[var(--color-text-secondary)]">
            {t('tasks.mapping.row')} {index + 1}
          </legend>
          <div className="grid gap-3 sm:grid-cols-2">
            <FormInput
              label={t('tasks.mapping.sourceSchema')}
              value={mapping.source_schema ?? ''}
              onChange={(value) => updateMapping(index, 'source_schema', value)}
              placeholder={t('tasks.mapping.schemaPlaceholder')}
            />
            <FormInput
              label={t('tasks.mapping.targetSchema')}
              value={mapping.target_schema ?? ''}
              onChange={(value) => updateMapping(index, 'target_schema', value)}
              placeholder={t('tasks.mapping.schemaPlaceholder')}
            />
            <FormInput
              label={t('tasks.mapping.sourceTable')}
              required
              value={mapping.source_table}
              onChange={(value) => updateMapping(index, 'source_table', value)}
              placeholder="customers"
            />
            <FormInput
              label={t('tasks.mapping.targetTable')}
              required
              value={mapping.target_table}
              onChange={(value) => updateMapping(index, 'target_table', value)}
              placeholder="customers"
            />
          </div>
          <button
            type="button"
            disabled={mappings.length === 1}
            onClick={() => onChange(mappings.filter((_, i) => i !== index))}
            className="flex items-center gap-1.5 text-xs text-[var(--color-error)] disabled:opacity-30"
            aria-label={`${t('tasks.mapping.remove')} ${index + 1}`}
          >
            <Trash2 size={14} />
            {t('tasks.mapping.remove')}
          </button>
        </fieldset>
      ))}
      {duplicateTarget && (
        <p role="alert" className="text-xs text-[var(--color-error)]">
          {t('tasks.mapping.duplicateTarget')}
        </p>
      )}
      <button
        type="button"
        disabled={mappings.length >= 100}
        onClick={() => onChange([...mappings, createEmptyTableMapping()])}
        className="flex items-center gap-1.5 rounded-lg border border-[var(--color-border-hover)] px-3 py-2 text-sm text-[var(--color-text-secondary)] disabled:opacity-30"
      >
        <Plus size={16} />
        {t('tasks.mapping.add')}
      </button>
      <div className="space-y-2 border-t border-[var(--color-border-default)] pt-4">
        <p className="text-sm text-[var(--color-text-secondary)]">
          {t('tasks.mapping.syncMode')}: {t('tasks.mapping.full')}
        </p>
        <p className="text-xs text-[var(--color-text-tertiary)]">{t('tasks.mapping.fullHint')}</p>
        <p className="text-xs text-[var(--color-text-tertiary)]">{t('tasks.mapping.appendHint')}</p>
        <FormInput
          label={t('tasks.mapping.batchSize')}
          type="number"
          value={batchSize}
          onChange={onBatchSizeChange}
          placeholder="1000"
        />
        <p className="text-xs text-[var(--color-text-tertiary)]">{t('tasks.mapping.batchHint')}</p>
        {batchSize !== '' &&
          (!Number.isInteger(Number(batchSize)) ||
            Number(batchSize) < 1 ||
            Number(batchSize) > 100000) && (
            <p role="alert" className="text-xs text-[var(--color-error)]">
              {t('tasks.mapping.batchInvalid')}
            </p>
          )}
      </div>
    </div>
  );
}
