import { useTranslation } from 'react-i18next';
import { ErrorBanner } from '../../../components/ui';
import type { TableMapping } from '../../../lib/validations';

type Props = {
  name: string;
  sourceName: string;
  targetName: string;
  cronExpr: string;
  mappings: TableMapping[];
  batchSize: number;
  error: string;
  submitting: boolean;
  onSubmit: () => void;
};

export function ConfirmStep({
  name,
  sourceName,
  targetName,
  cronExpr,
  mappings,
  batchSize,
  error,
  submitting,
  onSubmit,
}: Props) {
  const { t } = useTranslation();
  return (
    <div className="space-y-4">
      <h3 className="text-sm font-medium text-[var(--color-text-secondary)]">
        {t('tasks.confirmTitle')}
      </h3>
      <div className="space-y-3 rounded-lg border border-[var(--color-border-hover)] bg-[var(--color-bg-tertiary)]/30 p-4">
        <div className="flex justify-between text-sm">
          <span className="text-[var(--color-text-tertiary)]">{t('tasks.taskName')}</span>
          <span className="text-[var(--color-text-primary)]">{name}</span>
        </div>
        <div className="flex justify-between text-sm">
          <span className="text-[var(--color-text-tertiary)]">{t('tasks.source')}</span>
          <span className="text-[var(--color-text-primary)]">{sourceName}</span>
        </div>
        <div className="flex justify-between text-sm">
          <span className="text-[var(--color-text-tertiary)]">{t('tasks.target')}</span>
          <span className="text-[var(--color-text-primary)]">{targetName}</span>
        </div>
        <div className="flex justify-between text-sm">
          <span className="text-[var(--color-text-tertiary)]">{t('tasks.schedule')}</span>
          <span className="text-[var(--color-text-primary)]">
            {cronExpr || t('tasks.manualTrigger')}
          </span>
        </div>
        <div className="flex justify-between text-sm">
          <span className="text-[var(--color-text-tertiary)]">{t('tasks.mapping.syncMode')}</span>
          <span className="text-[var(--color-text-primary)]">{t('tasks.mapping.full')}</span>
        </div>
        <div className="flex justify-between text-sm">
          <span className="text-[var(--color-text-tertiary)]">{t('tasks.mapping.batchSize')}</span>
          <span className="text-[var(--color-text-primary)]">{batchSize}</span>
        </div>
        <div className="space-y-2 border-t border-[var(--color-border-default)] pt-3 text-sm">
          <p className="text-[var(--color-text-tertiary)]">{t('tasks.mappingConfig')}</p>
          {mappings.map((mapping, index) => (
            <p key={index} className="break-all font-mono text-xs text-[var(--color-text-primary)]">
              {[mapping.source_schema, mapping.source_table].filter(Boolean).join('.')} →{' '}
              {[mapping.target_schema, mapping.target_table].filter(Boolean).join('.')}
            </p>
          ))}
        </div>
      </div>
      <p className="text-xs text-[var(--color-text-tertiary)]">{t('tasks.mapping.appendHint')}</p>
      {error && <ErrorBanner message={error} />}
      <button
        onClick={onSubmit}
        disabled={submitting}
        className="w-full rounded-lg bg-[var(--color-action)] px-4 py-2.5 text-sm font-medium text-white shadow-sm transition-all hover:bg-[var(--color-action-hover)] disabled:opacity-50"
      >
        {submitting ? t('tasks.creating') : t('tasks.confirmCreate')}
      </button>
    </div>
  );
}
