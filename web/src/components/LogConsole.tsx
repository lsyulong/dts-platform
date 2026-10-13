import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

interface LogEntry {
  timestamp: string;
  level: string;
  component: string;
  message: string;
}

interface LogConsoleProps {
  title?: string;
  logs?: LogEntry[];
  maxHeight?: string;
  hideLevelFilters?: boolean;
}

const levelColors: Record<string, string> = {
  INFO: 'text-[var(--color-accent)]',
  WARN: 'text-[var(--color-warning)]',
  ERROR: 'text-[var(--color-error)]',
  DEBUG: 'text-[var(--color-text-tertiary)]',
  FATAL: 'text-red-600',
};

export default function LogConsole({
  title,
  logs = [],
  maxHeight = '300px',
  hideLevelFilters = false,
}: LogConsoleProps) {
  const { t } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [filter, setFilter] = useState('ALL');

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  const filteredLogs =
    hideLevelFilters || filter === 'ALL' ? logs : logs.filter((l) => l.level === filter);
  const displayTitle = title || t('dashboard.realtimeLog');

  return (
    <div className="rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] backdrop-blur-sm">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-[var(--color-border-default)] px-4 py-3">
        <h3 className="text-sm font-medium text-[var(--color-text-secondary)]">{displayTitle}</h3>
        {!hideLevelFilters && (
          <div className="flex gap-2">
            {['ALL', 'INFO', 'WARN', 'ERROR'].map((level) => (
              <button
                key={level}
                onClick={() => setFilter(level)}
                className={`rounded-lg px-2 py-0.5 text-xs font-medium transition-colors ${
                  filter === level
                    ? 'bg-[var(--color-bg-tertiary)] text-[var(--color-text-primary)]'
                    : 'text-[var(--color-text-tertiary)] hover:text-[var(--color-text-secondary)]'
                }`}
              >
                {level}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Log content */}
      <div
        ref={scrollRef}
        className="overflow-y-auto p-4 font-mono text-xs leading-relaxed"
        style={{ maxHeight }}
      >
        {filteredLogs.length === 0 ? (
          <p className="text-[var(--color-text-disabled)]">{t('dashboard.waitingForLogs')}</p>
        ) : (
          filteredLogs.map((log, i) => (
            <div key={i} className="flex gap-3 py-0.5">
              <span className="shrink-0 text-[var(--color-text-disabled)]">{log.timestamp}</span>
              <span
                className={`shrink-0 font-medium ${levelColors[log.level] || 'text-[var(--color-text-secondary)]'}`}
              >
                [{log.level}]
              </span>
              <span className="text-[var(--color-text-tertiary)]">[{log.component}]</span>
              <span className="text-[var(--color-text-secondary)]">{log.message}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
