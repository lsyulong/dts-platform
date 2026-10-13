import { useTranslation } from 'react-i18next';
export function ScheduleStep() {
  const { t } = useTranslation();
  return (
    <div className="space-y-4">
      <p className="text-sm text-[var(--color-text-secondary)]">{t('tasks.manualTrigger')}</p>
      <p className="text-xs text-[var(--color-text-tertiary)]">{t('tasks.manualRunHint')}</p>
    </div>
  );
}
