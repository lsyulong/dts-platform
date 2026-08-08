import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useDatasources, useCreateTask } from '../../../hooks/useApi';
import { taskCreateSchema, getFirstError } from '../../../lib/validations';
import { StepIndicator } from './StepIndicator';
import { BasicInfoStep } from './BasicInfoStep';
import { SourceStep } from './SourceStep';
import { TargetStep } from './TargetStep';
import { MappingStep } from './MappingStep';
import { ScheduleStep } from './ScheduleStep';
import { ConfirmStep } from './ConfirmStep';

export function TaskCreatePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { data: datasources = [] } = useDatasources();
  const createTask = useCreateTask();

  const [currentStep, setCurrentStep] = useState(0);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [cronExpr, setCronExpr] = useState('');
  const [error, setError] = useState('');

  const handleSubmit = () => {
    setError('');

    const validation = taskCreateSchema.safeParse({
      name,
      description,
      source_id: sourceId,
      target_id: targetId,
      cron_expr: cronExpr || undefined,
    });
    if (!validation.success) {
      setError(getFirstError(validation) || t('tasks.createFailed'));
      return;
    }

    createTask.mutate(
      {
        ...validation.data,
        cron_expr: validation.data.cron_expr ?? null,
      },
      {
        onSuccess: () => navigate('/tasks'),
        onError: (err: unknown) => {
          const msg =
            (err as { response?: { data?: { message?: string } } })?.response?.data?.message;
          setError(msg || t('tasks.createFailed'));
        },
      }
    );
  };

  const submitting = createTask.isPending;

  const canNext = () => {
    switch (currentStep) {
      case 0:
        return name.trim() !== '';
      case 1:
        return sourceId !== '';
      case 2:
        return targetId !== '';
      case 3:
        return true;
      case 4:
        return true;
      default:
        return true;
    }
  };

  const sourceName = datasources.find((d) => d.id === sourceId)?.name || '-';
  const targetName = datasources.find((d) => d.id === targetId)?.name || '-';

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-xl font-bold text-[var(--color-text-primary)]">{t('tasks.createTitle')}</h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">{t('tasks.createSubtitle')}</p>
      </div>

      {/* Step indicator */}
      <StepIndicator currentStep={currentStep} />

      {/* Step content */}
      <div className="rounded-2xl border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-6 backdrop-blur-sm">
        {currentStep === 0 && (
          <BasicInfoStep
            name={name}
            description={description}
            onNameChange={setName}
            onDescriptionChange={setDescription}
          />
        )}
        {currentStep === 1 && (
          <SourceStep
            datasources={datasources}
            selectedId={sourceId}
            onSelect={setSourceId}
          />
        )}
        {currentStep === 2 && (
          <TargetStep
            datasources={datasources}
            selectedId={targetId}
            excludeId={sourceId}
            onSelect={setTargetId}
          />
        )}
        {currentStep === 3 && <MappingStep />}
        {currentStep === 4 && (
          <ScheduleStep cronExpr={cronExpr} onChange={setCronExpr} />
        )}
        {currentStep === 5 && (
          <ConfirmStep
            name={name}
            sourceName={sourceName}
            targetName={targetName}
            cronExpr={cronExpr}
            error={error}
            submitting={submitting}
            onSubmit={handleSubmit}
          />
        )}
      </div>

      {/* Navigation buttons */}
      <div className="flex justify-between">
        <button
          onClick={() => setCurrentStep((s) => Math.max(0, s - 1))}
          disabled={currentStep === 0}
          className="flex items-center gap-1.5 rounded-xl border border-[var(--color-border-hover)] px-4 py-2.5 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] disabled:opacity-30"
        >
          <ChevronLeft size={16} />
          {t('tasks.prevStep')}
        </button>

        {currentStep < 5 && (
          <button
            onClick={() => setCurrentStep((s) => Math.min(5, s + 1))}
            disabled={!canNext()}
            className="flex items-center gap-1.5 rounded-xl bg-blue-500 px-4 py-2.5 text-sm font-medium text-white shadow-[0_0_20px_rgba(37,99,235,0.2)] transition-all hover:bg-blue-600 disabled:opacity-30"
          >
            {t('tasks.nextStep')}
            <ChevronRight size={16} />
          </button>
        )}
      </div>
    </div>
  );
}
