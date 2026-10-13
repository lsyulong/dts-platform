import { useTranslation } from 'react-i18next';
import { Check } from 'lucide-react';

type Props = {
  currentStep: number;
};

export function StepIndicator({ currentStep }: Props) {
  const { t } = useTranslation();
  const steps = [
    t('tasks.steps.basic'),
    t('tasks.steps.source'),
    t('tasks.steps.target'),
    t('tasks.steps.mapping'),
    t('tasks.steps.schedule'),
    t('tasks.steps.confirm'),
  ];

  return (
    <div className="flex items-center gap-2">
      {steps.map((label, i) => (
        <div key={i} className="flex items-center gap-2">
          <div
            className={`flex h-8 w-8 items-center justify-center rounded-full text-xs font-medium transition-all ${
              i < currentStep
                ? 'bg-emerald-500 text-white'
                : i === currentStep
                  ? 'bg-[var(--color-action)] text-white shadow-sm'
                  : 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-tertiary)]'
            }`}
          >
            {i < currentStep ? <Check size={14} /> : i + 1}
          </div>
          <span
            className={`text-xs ${i <= currentStep ? 'text-[var(--color-text-secondary)]' : 'text-[var(--color-text-disabled)]'}`}
          >
            {label}
          </span>
          {i < steps.length - 1 && <div className="h-px w-6 bg-[var(--color-bg-tertiary)]" />}
        </div>
      ))}
    </div>
  );
}
