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
}

const levelColors: Record<string, string> = {
  INFO: 'text-blue-400',
  WARN: 'text-amber-400',
  ERROR: 'text-rose-400',
  DEBUG: 'text-zinc-500',
  FATAL: 'text-red-600',
};

export default function LogConsole({ title, logs = [], maxHeight = '300px' }: LogConsoleProps) {
  const { t } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [filter, setFilter] = useState('ALL');

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  const filteredLogs = filter === 'ALL' ? logs : logs.filter((l) => l.level === filter);
  const displayTitle = title || t('dashboard.realtimeLog');

  return (
    <div className="rounded-2xl border border-zinc-800/60 bg-zinc-900/40 backdrop-blur-sm">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-zinc-800/60 px-4 py-3">
        <h3 className="text-sm font-medium text-zinc-300">{displayTitle}</h3>
        <div className="flex gap-2">
          {['ALL', 'INFO', 'WARN', 'ERROR'].map((level) => (
            <button
              key={level}
              onClick={() => setFilter(level)}
              className={`rounded-lg px-2 py-0.5 text-xs font-medium transition-colors ${
                filter === level
                  ? 'bg-zinc-700 text-zinc-200'
                  : 'text-zinc-500 hover:text-zinc-300'
              }`}
            >
              {level}
            </button>
          ))}
        </div>
      </div>

      {/* Log content */}
      <div
        ref={scrollRef}
        className="overflow-y-auto p-4 font-mono text-xs leading-relaxed"
        style={{ maxHeight }}
      >
        {filteredLogs.length === 0 ? (
          <p className="text-zinc-600">{t('dashboard.waitingForLogs')}</p>
        ) : (
          filteredLogs.map((log, i) => (
            <div key={i} className="flex gap-3 py-0.5">
              <span className="shrink-0 text-zinc-600">{log.timestamp}</span>
              <span className={`shrink-0 font-medium ${levelColors[log.level] || 'text-zinc-400'}`}>
                [{log.level}]
              </span>
              <span className="text-zinc-500">[{log.component}]</span>
              <span className="text-zinc-300">{log.message}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
