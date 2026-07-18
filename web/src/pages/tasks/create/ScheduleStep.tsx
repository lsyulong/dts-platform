import { useTranslation } from 'react-i18next';
import { FormInput } from '../../../components/ui';

type Props = {
  cronExpr: string;
  onChange: (v: string) => void;
};

export function ScheduleStep({ cronExpr, onChange }: Props) {
  const { t } = useTranslation();
  return (
    <div className="space-y-4">
      <FormInput
        label={`Cron (${t('common.cancel')})`}
        value={cronExpr}
        onChange={onChange}
        placeholder={t('tasks.cronPlaceholder')}
      />
      <p className="mt-1 text-xs text-[var(--color-text-disabled)]">{t('tasks.cronHint')}</p>
    </div>
  );
}
