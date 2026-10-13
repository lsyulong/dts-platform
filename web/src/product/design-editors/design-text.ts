import { useTranslation } from 'react-i18next';

export function useDesignText() {
  const { i18n } = useTranslation();
  return (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
}
export const designField =
  'flex min-w-0 flex-col gap-1.5 text-xs text-[var(--color-text-secondary)]';
export const designInput =
  'min-h-9 min-w-0 rounded-md border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] px-2.5 py-2 text-sm text-[var(--color-text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--color-accent)] disabled:opacity-50';
export const designPanel =
  'min-w-0 rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-secondary)] p-4';
