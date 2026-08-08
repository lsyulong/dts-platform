import { useTranslation } from 'react-i18next';
import { ErrorBanner } from '../../../components/ui';

type Props = {
  name: string;
  sourceName: string;
  targetName: string;
  cronExpr: string;
  error: string;
  submitting: boolean;
  onSubmit: () => void;
};

export function ConfirmStep({
  name,
  sourceName,
  targetName,
  cronExpr,
  error,
  submitting,
  onSubmit,
}: Props) {
  const { t } = useTranslation();
  return (
    <div className="space-y-4">
      <h3 className="text-sm font-medium text-[var(--color-text-secondary)]">{t('tasks.confirmTitle')}</h3>
      <div className="space-y-3 rounded-xl border border-[var(--color-border-hover)] bg-[var(--color-bg-tertiary)]/30 p-4">
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
          <span className="text-[var(--color-text-primary)]">{cronExpr || t('tasks.manualTrigger')}</span>
        </div>
      </div>
      {error && <ErrorBanner message={error} />}
      <button
        onClick={onSubmit}
        disabled={submitting}
        className="w-full rounded-xl bg-blue-500 px-4 py-2.5 text-sm font-medium text-white shadow-[0_0_20px_rgba(37,99,235,0.2)] transition-all hover:bg-blue-600 disabled:opacity-50"
      >
        {submitting ? t('tasks.creating') : t('tasks.confirmCreate')}
      </button>
    </div>
  );
}
