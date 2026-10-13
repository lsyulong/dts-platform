import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useDatasources, useCreateTask, useDatasourceTables } from '../../../hooks/useApi';
import {
  taskCreateSchema,
  tableMappingsSchema,
  getFirstError,
  createEmptyTableMapping,
  type TableMapping,
} from '../../../lib/validations';
import { StepIndicator } from './StepIndicator';
import { BasicInfoStep } from './BasicInfoStep';
import { SourceStep } from './SourceStep';
import { TargetStep } from './TargetStep';
import { MappingStep } from './MappingStep';
import { ScheduleStep } from './ScheduleStep';
import { ConfirmStep } from './ConfirmStep';
import { apiErrorMessage } from '../../../lib/apiError';
import { useAuthStore } from '../../../stores/auth';
import { ErrorBanner } from '../../../components/ui';

export function TaskCreatePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { data: datasources = [] } = useDatasources();
  const createTask = useCreateTask();
  const user = useAuthStore((state) => state.user);
  const canWrite = user?.permissions.includes('task:write') ?? false;

  const [currentStep, setCurrentStep] = useState(0);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [error, setError] = useState('');
  const [mappings, setMappings] = useState<TableMapping[]>([createEmptyTableMapping()]);
  const [batchSize, setBatchSize] = useState('');
  const { data: sourceTables = [], isLoading: sourceTablesLoading } = useDatasourceTables(
    sourceId || undefined,
  );

  const handleSubmit = () => {
    if (!canWrite) return;
    setError('');

    const validation = taskCreateSchema.safeParse({
      name,
      description,
      source_id: sourceId,
      target_id: targetId,
      table_mappings: mappings,
      sync_mode: 'full',
      batch_size: batchSize === '' ? undefined : Number(batchSize),
    });
    if (!validation.success) {
      setError(
        t(`tasks.validation.${getFirstError(validation) ?? 'required'}`, {
          defaultValue: t('errors.validationError'),
        }),
      );
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
          setError(apiErrorMessage(err, t('tasks.createFailed')));
        },
      },
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
        return targetId !== '' && sourceId !== targetId;
      case 3:
        return (
          tableMappingsSchema.safeParse(mappings).success &&
          (batchSize === '' ||
            (Number.isInteger(Number(batchSize)) &&
              Number(batchSize) >= 1 &&
              Number(batchSize) <= 100000))
        );
      case 4:
        return true;
      default:
        return true;
    }
  };

  const sourceName = datasources.find((d) => d.id === sourceId)?.name || '-';
  const targetName = datasources.find((d) => d.id === targetId)?.name || '-';
  const supportedDatasources = datasources.filter((datasource) =>
    ['mysql', 'postgresql', 'postgres', 'pg', 'duckdb', 'swanlake'].includes(
      datasource.db_type.toLowerCase(),
    ),
  );
  const selectedSourceType = supportedDatasources
    .find((datasource) => datasource.id === sourceId)
    ?.db_type.toLowerCase();
  const targetDatasources = supportedDatasources.filter((datasource) => {
    const targetType = datasource.db_type.toLowerCase();
    const sourceIsDuckDb = ['duckdb', 'swanlake'].includes(selectedSourceType ?? '');
    return !sourceIsDuckDb || ['duckdb', 'swanlake'].includes(targetType);
  });

  if (!canWrite) return <ErrorBanner message={t('errors.forbidden')} />;

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <div>
        <h1 className="text-xl font-bold text-[var(--color-text-primary)]">
          {t('tasks.createTitle')}
        </h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {t('tasks.createSubtitle')}
        </p>
      </div>

      <StepIndicator currentStep={currentStep} />

      <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-6 backdrop-blur-sm">
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
            datasources={supportedDatasources}
            selectedId={sourceId}
            onSelect={(id) => {
              if (id !== sourceId) {
                setMappings([createEmptyTableMapping()]);
              }
              setSourceId(id);
              const newSourceType = datasources
                .find((datasource) => datasource.id === id)
                ?.db_type.toLowerCase();
              const currentTargetType = datasources
                .find((datasource) => datasource.id === targetId)
                ?.db_type.toLowerCase();
              if (
                id === targetId ||
                (['duckdb', 'swanlake'].includes(newSourceType ?? '') &&
                  !['duckdb', 'swanlake'].includes(currentTargetType ?? ''))
              ) {
                setTargetId('');
              }
            }}
          />
        )}
        {currentStep === 2 && (
          <TargetStep
            datasources={targetDatasources}
            selectedId={targetId}
            excludeId={sourceId}
            onSelect={setTargetId}
          />
        )}
        {currentStep === 3 && (
          <MappingStep
            mappings={mappings}
            batchSize={batchSize}
            availableTables={sourceTables}
            loadingTables={sourceTablesLoading}
            onChange={setMappings}
            onBatchSizeChange={setBatchSize}
          />
        )}
        {currentStep === 4 && <ScheduleStep />}
        {currentStep === 5 && (
          <ConfirmStep
            name={name}
            sourceName={sourceName}
            targetName={targetName}
            cronExpr=""
            mappings={tableMappingsSchema.safeParse(mappings).data ?? []}
            batchSize={batchSize ? Number(batchSize) : 1000}
            error={error}
            submitting={submitting}
            onSubmit={handleSubmit}
          />
        )}
      </div>

      <div className="flex justify-between">
        <button
          onClick={() => setCurrentStep((s) => Math.max(0, s - 1))}
          disabled={currentStep === 0 || submitting}
          className="flex items-center gap-1.5 rounded-lg border border-[var(--color-border-hover)] px-4 py-2.5 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] disabled:opacity-30"
        >
          <ChevronLeft size={16} />
          {t('tasks.prevStep')}
        </button>

        {currentStep < 5 && (
          <button
            onClick={() => setCurrentStep((s) => Math.min(5, s + 1))}
            disabled={!canNext()}
            className="flex items-center gap-1.5 rounded-lg bg-[var(--color-action)] px-4 py-2.5 text-sm font-medium text-white shadow-sm transition-all hover:bg-[var(--color-action-hover)] disabled:opacity-30"
          >
            {t('tasks.nextStep')}
            <ChevronRight size={16} />
          </button>
        )}
      </div>
    </div>
  );
}
