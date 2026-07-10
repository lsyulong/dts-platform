import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ChevronLeft, ChevronRight, Check, Database } from 'lucide-react';
import client from '../../api/client';

interface Datasource {
  id: string;
  name: string;
  db_type: string;
  host: string;
  port: number;
}

export default function TaskCreatePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [currentStep, setCurrentStep] = useState(0);
  const [datasources, setDatasources] = useState<Datasource[]>([]);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [cronExpr, setCronExpr] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState('');

  const steps = [
    t('tasks.steps.basic'),
    t('tasks.steps.source'),
    t('tasks.steps.target'),
    t('tasks.steps.mapping'),
    t('tasks.steps.schedule'),
    t('tasks.steps.confirm'),
  ];

  useEffect(() => {
    client.get('/datasources').then((res) => setDatasources(res.data.data || []));
  }, []);

  const handleSubmit = async () => {
    setSubmitting(true);
    setError('');
    try {
      await client.post('/tasks', {
        name,
        description,
        source_id: sourceId,
        target_id: targetId,
        cron_expr: cronExpr || null,
      });
      navigate('/tasks');
    } catch (err: unknown) {
      setError((err as { response?: { data?: { message?: string } } })?.response?.data?.message || t('tasks.createFailed'));
    } finally {
      setSubmitting(false);
    }
  };

  const canNext = () => {
    switch (currentStep) {
      case 0: return name.trim() !== '';
      case 1: return sourceId !== '';
      case 2: return targetId !== '';
      case 3: return true;
      case 4: return true;
      default: return true;
    }
  };

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-xl font-bold text-zinc-100">{t('tasks.createTitle')}</h1>
        <p className="mt-1 text-sm text-zinc-500">{t('tasks.createSubtitle')}</p>
      </div>

      {/* Step indicator */}
      <div className="flex items-center gap-2">
        {steps.map((label, i) => (
          <div key={i} className="flex items-center gap-2">
            <div
              className={`flex h-8 w-8 items-center justify-center rounded-full text-xs font-medium transition-all ${
                i < currentStep
                  ? 'bg-emerald-500 text-white'
                  : i === currentStep
                    ? 'bg-blue-500 text-white shadow-[0_0_12px_rgba(37,99,235,0.3)]'
                    : 'bg-zinc-800 text-zinc-500'
              }`}
            >
              {i < currentStep ? <Check size={14} /> : i + 1}
            </div>
            <span className={`text-xs ${i <= currentStep ? 'text-zinc-300' : 'text-zinc-600'}`}>
              {label}
            </span>
            {i < steps.length - 1 && <div className="h-px w-6 bg-zinc-800" />}
          </div>
        ))}
      </div>

      {/* Step content */}
      <div className="rounded-2xl border border-zinc-800/60 bg-zinc-900/40 p-6 backdrop-blur-sm">
        {/* Step 0: Basic info */}
        {currentStep === 0 && (
          <div className="space-y-4">
            <div>
              <label className="mb-1.5 block text-sm font-medium text-zinc-400">{t('tasks.taskName')}</label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t('tasks.namePlaceholder')}
                className="w-full rounded-xl border border-zinc-700 bg-zinc-800/50 px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-zinc-400">{t('tasks.descPlaceholder')}</label>
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                rows={3}
                placeholder={t('tasks.descPlaceholder')}
                className="w-full rounded-xl border border-zinc-700 bg-zinc-800/50 px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
            </div>
          </div>
        )}

        {/* Step 1: Source */}
        {currentStep === 1 && (
          <div className="space-y-3">
            <p className="text-sm text-zinc-400">{t('tasks.selectSource')}</p>
            {datasources.map((ds) => (
              <button
                key={ds.id}
                onClick={() => setSourceId(ds.id)}
                className={`w-full rounded-xl border p-4 text-left transition-all ${
                  sourceId === ds.id
                    ? 'border-blue-500 bg-blue-500/10'
                    : 'border-zinc-700 bg-zinc-800/30 hover:border-zinc-600'
                }`}
              >
                <div className="flex items-center gap-3">
                  <Database size={20} className="text-zinc-400" />
                  <div>
                    <p className="text-sm font-medium text-zinc-200">{ds.name}</p>
                    <p className="text-xs text-zinc-500">{ds.db_type} @ {ds.host}:{ds.port}</p>
                  </div>
                </div>
              </button>
            ))}
          </div>
        )}

        {/* Step 2: Target */}
        {currentStep === 2 && (
          <div className="space-y-3">
            <p className="text-sm text-zinc-400">{t('tasks.selectTarget')}</p>
            {datasources
              .filter((ds) => ds.id !== sourceId)
              .map((ds) => (
                <button
                  key={ds.id}
                  onClick={() => setTargetId(ds.id)}
                  className={`w-full rounded-xl border p-4 text-left transition-all ${
                    targetId === ds.id
                      ? 'border-blue-500 bg-blue-500/10'
                      : 'border-zinc-700 bg-zinc-800/30 hover:border-zinc-600'
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <Database size={20} className="text-zinc-400" />
                    <div>
                      <p className="text-sm font-medium text-zinc-200">{ds.name}</p>
                      <p className="text-xs text-zinc-500">{ds.db_type} @ {ds.host}:{ds.port}</p>
                    </div>
                  </div>
                </button>
              ))}
          </div>
        )}

        {/* Step 3: Field mapping (placeholder) */}
        {currentStep === 3 && (
          <div className="flex flex-col items-center justify-center py-10 text-center">
            <Database size={40} className="text-zinc-600" />
            <p className="mt-4 text-sm text-zinc-400">{t('tasks.mappingConfig')}</p>
            <p className="mt-1 text-xs text-zinc-600">{t('tasks.mappingHint')}</p>
          </div>
        )}

        {/* Step 4: Schedule */}
        {currentStep === 4 && (
          <div className="space-y-4">
            <div>
              <label className="mb-1.5 block text-sm font-medium text-zinc-400">
                Cron <span className="text-zinc-600">({t('common.cancel')})</span>
              </label>
              <input
                type="text"
                value={cronExpr}
                onChange={(e) => setCronExpr(e.target.value)}
                placeholder={t('tasks.cronPlaceholder')}
                className="w-full rounded-xl border border-zinc-700 bg-zinc-800/50 px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
              <p className="mt-1 text-xs text-zinc-600">{t('tasks.cronHint')}</p>
            </div>
          </div>
        )}

        {/* Step 5: Confirm */}
        {currentStep === 5 && (
          <div className="space-y-4">
            <h3 className="text-sm font-medium text-zinc-300">{t('tasks.confirmTitle')}</h3>
            <div className="space-y-3 rounded-xl border border-zinc-700 bg-zinc-800/30 p-4">
              <div className="flex justify-between text-sm">
                <span className="text-zinc-500">{t('tasks.taskName')}</span>
                <span className="text-zinc-200">{name}</span>
              </div>
              <div className="flex justify-between text-sm">
                <span className="text-zinc-500">{t('tasks.source')}</span>
                <span className="text-zinc-200">{datasources.find((d) => d.id === sourceId)?.name || '-'}</span>
              </div>
              <div className="flex justify-between text-sm">
                <span className="text-zinc-500">{t('tasks.target')}</span>
                <span className="text-zinc-200">{datasources.find((d) => d.id === targetId)?.name || '-'}</span>
              </div>
              <div className="flex justify-between text-sm">
                <span className="text-zinc-500">{t('tasks.schedule')}</span>
                <span className="text-zinc-200">{cronExpr || t('tasks.manualTrigger')}</span>
              </div>
            </div>
            {error && (
              <div className="rounded-lg border border-red-500/20 bg-red-500/10 px-4 py-2.5 text-sm text-red-400">
                {error}
              </div>
            )}
            <button
              onClick={handleSubmit}
              disabled={submitting}
              className="w-full rounded-xl bg-blue-500 px-4 py-2.5 text-sm font-medium text-white shadow-[0_0_20px_rgba(37,99,235,0.2)] transition-all hover:bg-blue-600 disabled:opacity-50"
            >
              {submitting ? t('tasks.creating') : t('tasks.confirmCreate')}
            </button>
          </div>
        )}
      </div>

      {/* Navigation buttons */}
      <div className="flex justify-between">
        <button
          onClick={() => setCurrentStep((s) => Math.max(0, s - 1))}
          disabled={currentStep === 0}
          className="flex items-center gap-1.5 rounded-xl border border-zinc-700 px-4 py-2.5 text-sm text-zinc-400 hover:bg-zinc-800 disabled:opacity-30"
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
