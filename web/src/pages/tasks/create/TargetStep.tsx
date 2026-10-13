import { useTranslation } from 'react-i18next';
import { Database } from 'lucide-react';

interface Datasource {
  id: string;
  name: string;
  db_type: string;
  host: string;
  port: number;
}

type Props = {
  datasources: Datasource[];
  selectedId: string;
  excludeId: string;
  onSelect: (id: string) => void;
};

export function TargetStep({ datasources, selectedId, excludeId, onSelect }: Props) {
  const { t } = useTranslation();
  const filtered = datasources.filter((ds) => ds.id !== excludeId);
  return (
    <div className="space-y-3">
      <p className="text-sm text-[var(--color-text-secondary)]">{t('tasks.selectTarget')}</p>
      <p className="text-xs text-[var(--color-text-tertiary)]">{t('tasks.supportedDatasources')}</p>
      {filtered.map((ds) => (
        <button
          key={ds.id}
          onClick={() => onSelect(ds.id)}
          className={`w-full rounded-lg border p-4 text-left transition-all ${
            selectedId === ds.id
              ? 'border-[var(--color-accent)] bg-[var(--color-accent-subtle)]'
              : 'border-[var(--color-border-hover)] bg-[var(--color-bg-tertiary)]/30 hover:border-[var(--color-border-hover)]'
          }`}
        >
          <div className="flex items-center gap-3">
            <Database size={20} className="text-[var(--color-text-secondary)]" />
            <div>
              <p className="text-sm font-medium text-[var(--color-text-primary)]">{ds.name}</p>
              <p className="text-xs text-[var(--color-text-tertiary)]">
                {ds.db_type} @ {ds.host}:{ds.port}
              </p>
            </div>
          </div>
        </button>
      ))}
    </div>
  );
}
