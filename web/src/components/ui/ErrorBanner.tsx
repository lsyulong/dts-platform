import { useTranslation } from 'react-i18next';

type Props = {
  message: string;
  onRetry?: () => void;
  onDismiss?: () => void;
};

export function ErrorBanner({ message, onRetry, onDismiss }: Props) {
  const { t } = useTranslation();

  return (
    <div className="rounded-lg border border-red-500/20 bg-red-500/10 px-4 py-2.5 text-sm text-[var(--color-error)]">
      <div className="flex items-start justify-between gap-3">
        <span className="flex-1">{message}</span>
        <div className="flex items-center gap-2">
          {onRetry && (
            <button
              onClick={onRetry}
              className="whitespace-nowrap rounded-lg bg-[var(--color-bg-tertiary)] px-3 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)]"
            >
              {t('common.retry')}
            </button>
          )}
          {onDismiss && (
            <button
              onClick={onDismiss}
              className="text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)]"
              aria-label={t('common.close')}
            >
              ✕
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
