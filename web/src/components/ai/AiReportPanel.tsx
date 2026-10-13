import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { FileText, Loader2 } from 'lucide-react';
import client from '../../api/client';
import { readSseText } from '../../lib/readSseText';
import { clearSession } from '../../lib/session';

type Props = {
  taskId: string;
};

export function AiReportPanel({ taskId }: Props) {
  const { t } = useTranslation();
  const [report, setReport] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [generated, setGenerated] = useState(false);
  const [disabled, setDisabled] = useState(false);

  const generate = useCallback(async () => {
    setLoading(true);
    setError(null);
    setReport('');
    setGenerated(false);

    try {
      const token = localStorage.getItem('token');
      const response = await fetch(
        `${client.defaults.baseURL || '/api/v1'}/ai/report?task_id=${encodeURIComponent(taskId)}`,
        {
          method: 'POST',
          headers: {
            Authorization: `Bearer ${token}`,
            Accept: 'text/event-stream',
          },
        },
      );

      if (!response.ok) {
        if (response.status === 401) {
          clearSession(token);
          throw new Error(t('errors.unauthorized'));
        }
        const body = (await response.json().catch(() => null)) as { message?: string } | null;
        const message = body?.message || `HTTP ${response.status}`;
        if (/未启用|disabled/i.test(message)) {
          setDisabled(true);
          return;
        }
        throw new Error(message);
      }
      if (!response.body) throw new Error(t('ai.reportFailed'));
      await readSseText(response.body, setReport);
      setGenerated(true);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : t('ai.reportFailed');
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [taskId, t]);

  return (
    <div className="space-y-4">
      {disabled && (
        <p role="status" className="text-sm text-[var(--color-text-tertiary)]">
          {t('ai.disabled')}
        </p>
      )}
      {!generated && !loading && (
        <button
          type="button"
          onClick={generate}
          disabled={disabled}
          className="inline-flex items-center gap-2 rounded-lg bg-[var(--color-action)] px-4 py-2.5 text-sm font-medium text-white shadow-lg shadow-[var(--color-accent-glow)] transition-all hover:bg-[var(--color-action-hover)] active:scale-95 disabled:opacity-50"
        >
          <FileText size={16} />
          {t('ai.generateReport')}
        </button>
      )}

      {loading && (
        <div className="flex items-center gap-3 rounded-lg border border-[var(--color-border-default)] bg-[var(--color-accent-subtle)] p-4">
          <Loader2 size={18} className="animate-spin text-[var(--color-accent)]" />
          <span className="text-sm text-[var(--color-text-tertiary)]">
            {t('ai.generatingReport')}
          </span>
        </div>
      )}

      {error && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4">
          <p className="text-sm font-medium text-[var(--color-error)]">{t('ai.reportFailed')}</p>
          <p className="mt-1 text-xs text-[var(--color-error)]">{error}</p>
        </div>
      )}

      {report && (
        <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)]">
          <div className="border-b border-[var(--color-border-default)] px-5 py-3">
            <h3 className="text-sm font-semibold text-[var(--color-text-primary)] flex items-center gap-2">
              <FileText size={16} className="text-[var(--color-accent)]" />
              {t('ai.reportTitle')}
            </h3>
          </div>
          <div className="p-5 prose prose-sm max-w-none dark:prose-invert">
            <pre className="whitespace-pre-wrap break-words font-sans text-xs leading-relaxed text-[var(--color-text-secondary)]">
              {report}
            </pre>
          </div>
          {generated && (
            <div className="border-t border-[var(--color-border-default)] px-5 py-3 flex gap-2">
              <button
                type="button"
                onClick={generate}
                disabled={loading}
                className="text-xs text-[var(--color-accent)] hover:text-[var(--color-accent-hover)] font-medium"
              >
                {t('ai.regenerateReport')}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
