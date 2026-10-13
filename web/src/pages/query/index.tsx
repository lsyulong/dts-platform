import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import Editor, { type OnMount } from '@monaco-editor/react';
import {
  Play,
  Database,
  Clock,
  ChevronDown,
  Trash2,
  Loader2,
  BarChart3,
  LineChart,
  PieChart,
  type LucideIcon,
} from 'lucide-react';
import client from '../../api/client';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession, queryHistoryKey } from '../../lib/session';
import { useAuthStore } from '../../stores/auth';
import { useAppStore } from '../../stores/app';

type ColumnMeta = { name: string; data_type: string; is_numeric: boolean };
type ChartSuggestion = {
  chart_type: string;
  x_column: string | null;
  y_column: string | null;
  reason: string;
};
type QueryResult = {
  columns: ColumnMeta[];
  rows: Record<string, unknown>[];
  row_count: number;
  suggested_chart: ChartSuggestion | null;
};
type SchemaInfo = { catalog: string | null; schema: string; tables: { name: string }[] };

const CHART_ICONS: Record<string, LucideIcon> = {
  bar: BarChart3,
  line: LineChart,
  pie: PieChart,
};

const MAX_HISTORY = 50;

interface HistoryEntry {
  sql: string;
  timestamp: number;
}

function readHistory(key: string | null): HistoryEntry[] {
  if (!key) return [];
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key) || '[]');
    return Array.isArray(value)
      ? value
          .filter((entry) => typeof entry?.sql === 'string' && typeof entry?.timestamp === 'number')
          .slice(0, MAX_HISTORY)
      : [];
  } catch {
    return [];
  }
}

function cellText(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  return typeof value === 'object' ? JSON.stringify(value) : String(value);
}

export default function QueryPage() {
  const identity = useAuthStore((state) => JSON.stringify([
    state.token, state.user?.tenant_id, state.user?.id, state.user?.permissions,
  ]));
  return <QueryWorkspace key={identity} />;
}

function QueryWorkspace() {
  const { t } = useTranslation();
  const user = useAuthStore((state) => state.user);
  const theme = useAppStore((state) => state.theme);
  const [catalogVisible, setCatalogVisible] = useState(false);
  const historyKey = queryHistoryKey(user);

  const [sql, setSql] = useState('SELECT 1 AS hello');
  const [result, setResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [schemas, setSchemas] = useState<SchemaInfo[]>([]);
  const [schemaOpen, setSchemaOpen] = useState(false);
  const [schemaError, setSchemaError] = useState<string | null>(null);
  const [schemaLoading, setSchemaLoading] = useState(false);
  const [historyState, setHistoryState] = useState(() => ({
    key: historyKey,
    entries: readHistory(historyKey),
  }));
  const history = historyState.key === historyKey ? historyState.entries : readHistory(historyKey);
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const requests = useRef(new AbortController());
  const schemaRequest = useRef(0);

  useEffect(() => {
    const controller = new AbortController();
    requests.current = controller;
    return () => controller.abort();
  }, []);

  useEffect(() => {
    if (historyKey && historyState.key === historyKey)
      localStorage.setItem(historyKey, JSON.stringify(historyState.entries.slice(0, MAX_HISTORY)));
  }, [historyKey, historyState]);

  const executeQuery = useCallback(async () => {
    const trimmed = sql.trim();
    if (!trimmed || loading) return;
    setLoading(true);
    setError(null);
    setResult(null);
    const session = captureSession();
    const signal = requests.current.signal;
    try {
      const { data } = await client.post<QueryResult>(
        '/query/execute',
        { sql: trimmed },
        { timeout: 65000, signal, headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (signal.aborted || !isCurrentSession(session)) return;
      setResult(data);
      setHistoryState((prev) => ({
        key: historyKey,
        entries: [
          { sql: trimmed, timestamp: Date.now() },
          ...(prev.key === historyKey ? prev.entries : readHistory(historyKey)),
        ].slice(0, MAX_HISTORY),
      }));
    } catch (err: unknown) {
      if (!signal.aborted && isCurrentSession(session)) setError(apiErrorMessage(err, t('query.error')));
    } finally {
      if (!signal.aborted && isCurrentSession(session)) setLoading(false);
    }
  }, [sql, loading, t, historyKey]);

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
        event.preventDefault();
        void executeQuery();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [executeQuery]);

  const fetchSchemas = useCallback(async () => {
    const request = ++schemaRequest.current;
    const session = captureSession();
    const signal = requests.current.signal;
    setSchemaError(null);
    setSchemas([]);
    setSchemaLoading(true);
    try {
      const { data } = await client.get<SchemaInfo[]>('/query/schemas', { signal, headers: { Authorization: `Bearer ${session.token}` } });
      if (!signal.aborted && isCurrentSession(session) && request === schemaRequest.current) setSchemas(data);
    } catch (err) {
      if (!signal.aborted && isCurrentSession(session) && request === schemaRequest.current)
        setSchemaError(apiErrorMessage(err, t('query.schemaError')));
    } finally {
      if (!signal.aborted && isCurrentSession(session) && request === schemaRequest.current) setSchemaLoading(false);
    }
  }, [t]);

  const handleEditorMount: OnMount = useCallback((editor) => {
    editorRef.current = editor;
  }, []);

  const loadHistoryEntry = useCallback((entry: HistoryEntry) => {
    setSql(entry.sql);
  }, []);

  const clearHistory = useCallback(() => {
    setHistoryState({ key: historyKey, entries: [] });
    if (historyKey) localStorage.removeItem(historyKey);
  }, [historyKey]);

  const insertTableName = useCallback((catalog: string | null, schema: string, table: string) => {
    const qualifiedName = [catalog, schema, table]
      .filter((part): part is string => part !== null)
      .map((part) => `"${part.replace(/"/g, '""')}"`)
      .join('.');
    setSql((prev) => prev + qualifiedName);
    editorRef.current?.focus();
  }, []);

  return (
    <div className="data-panel flex h-[calc(100dvh-7rem)] min-h-[520px] flex-col">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--color-border-default)] px-4 py-4">
        <div>
          <h1 className="text-lg font-semibold text-[var(--color-text-primary)]">
            <Database className="mr-2 inline-block text-[var(--color-accent)]" size={22} />
            {t('nav.query')}
          </h1>
          <p className="mt-0.5 text-xs text-[var(--color-text-tertiary)]">{t('query.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            className="rounded-md border border-[var(--color-border-default)] px-3 py-2 text-xs lg:hidden"
            onClick={() => setCatalogVisible((shown) => !shown)}
            aria-expanded={catalogVisible}
          >
            {t('query.toggleCatalog')}
          </button>
          <button
            type="button"
            onClick={executeQuery}
            disabled={loading || !sql.trim()}
            className="inline-flex items-center gap-2 rounded-lg bg-[var(--color-action)] px-5 py-2.5 text-sm font-medium text-white shadow-lg shadow-[var(--color-accent-glow)] transition-all hover:bg-[var(--color-action-hover)] active:scale-95 disabled:opacity-50"
          >
            {loading ? <Loader2 size={16} className="animate-spin" /> : <Play size={16} />}
            {t('query.execute')}
          </button>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden lg:flex-row">
        <aside
          className={`${catalogVisible ? 'block' : 'hidden'} max-h-72 shrink-0 overflow-y-auto border-b border-[var(--color-border-default)] bg-[var(--color-bg-secondary)] p-3 lg:block lg:max-h-none lg:w-56 lg:border-b-0 lg:border-r`}
        >
          <button
            type="button"
            onClick={() => {
              setSchemaOpen(!schemaOpen);
              if (!schemaOpen) fetchSchemas();
            }}
            className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-xs font-medium text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)]"
          >
            <span className="flex items-center gap-2">
              <Database size={14} />
              {t('query.schemas')}
            </span>
            <ChevronDown
              size={14}
              className={`transition-transform ${schemaOpen ? 'rotate-180' : ''}`}
            />
          </button>

          {schemaOpen && schemaError && (
            <p role="alert" className="mt-2 px-3 text-[11px] text-[var(--color-error)]">
              {schemaError}
            </p>
          )}
          {schemaOpen && !schemaError && !schemaLoading && schemas.length === 0 && (
            <p className="mt-2 px-3 text-[11px] text-[var(--color-text-disabled)]">
              {t('query.noSchemas')}
            </p>
          )}

          {schemaOpen && !schemaError &&
            schemas.map((s) => (
              <div key={JSON.stringify([s.catalog, s.schema])} className="mt-2">
                <p className="px-3 text-[11px] font-semibold uppercase text-[var(--color-text-disabled)]">
                  {[s.catalog, s.schema].filter(Boolean).join('.')}
                </p>
                {s.tables.map((tbl) => (
                  <button
                    key={JSON.stringify([s.catalog, s.schema, tbl.name])}
                    type="button"
                    onClick={() => insertTableName(s.catalog, s.schema, tbl.name)}
                    className="flex w-full items-center gap-2 rounded-md px-5 py-1 text-left text-[11px] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                  >
                    {tbl.name}
                  </button>
                ))}
              </div>
            ))}

          <div className="mt-6">
            <div className="flex items-center justify-between px-3">
              <span className="flex items-center gap-2 text-xs font-medium text-[var(--color-text-secondary)]">
                <Clock size={14} />
                {t('query.history')}
              </span>
              {history.length > 0 && (
                <button
                  type="button"
                  onClick={clearHistory}
                  className="rounded p-0.5 text-[var(--color-text-disabled)] hover:text-[var(--color-error)]"
                  aria-label={t('query.clearHistory')}
                >
                  <Trash2 size={12} />
                </button>
              )}
            </div>
            <div className="mt-1 max-h-48 overflow-y-auto space-y-0.5">
              {history.slice(0, 20).map((entry, i) => (
                <button
                  key={`${entry.timestamp}-${i}`}
                  type="button"
                  onClick={() => loadHistoryEntry(entry)}
                  className="block w-full truncate rounded-md px-3 py-1 text-left text-[11px] text-[var(--color-text-tertiary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                  title={entry.sql}
                >
                  {entry.sql.slice(0, 60)}
                  {entry.sql.length > 60 ? '…' : ''}
                </button>
              ))}
            </div>
          </div>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <div className="h-48 shrink-0 border-b border-[var(--color-border-default)]">
            <Editor
              height="100%"
              defaultLanguage="sql"
              value={sql}
              onChange={(v) => setSql(v || '')}
              onMount={handleEditorMount}
              theme={theme === 'dark' ? 'vs-dark' : 'vs'}
              options={{
                minimap: { enabled: false },
                fontSize: 13,
                lineNumbers: 'on',
                scrollBeyondLastLine: false,
                wordWrap: 'on',
                padding: { top: 12, bottom: 12 },
                automaticLayout: true,
              }}
            />
          </div>

          <div className="flex-1 overflow-auto">
            {error && (
              <div className="m-4 rounded-lg border border-red-500/20 bg-red-500/5 p-4">
                <p className="text-sm font-medium text-[var(--color-error)]">{t('query.error')}</p>
                <pre className="mt-1 text-xs text-[var(--color-error)] whitespace-pre-wrap">
                  {error}
                </pre>
              </div>
            )}

            {loading && (
              <div className="flex items-center justify-center py-20">
                <Loader2 size={24} className="animate-spin text-[var(--color-accent)]" />
                <span className="ml-3 text-sm text-[var(--color-text-tertiary)]">
                  {t('query.executing')}
                </span>
              </div>
            )}

            {result && !loading && (
              <div className="p-4">
                <p className="mb-3 text-xs text-[var(--color-text-tertiary)]">
                  {t('query.rowsReturned', { count: result.row_count })}
                  {result.columns.length > 0 && ` · ${result.columns.length} ${t('query.columns')}`}
                </p>

                {result.suggested_chart &&
                  (() => {
                    const sc = result.suggested_chart;
                    const ChartIcon = CHART_ICONS[sc.chart_type] || BarChart3;
                    return (
                      <div className="mb-4 rounded-lg border border-[var(--color-border-default)] bg-[var(--color-accent-subtle)] p-3 flex items-center gap-3">
                        <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--color-accent-subtle)]">
                          <ChartIcon size={20} className="text-[var(--color-accent)]" />
                        </div>
                        <div className="flex-1">
                          <p className="text-xs font-medium text-[var(--color-text-primary)]">
                            {t('query.suggestedChart')}:{' '}
                            {t(`query.charts.${sc.chart_type}`, { defaultValue: sc.chart_type })}
                          </p>
                          <p className="text-[11px] text-[var(--color-text-tertiary)]">
                            {sc.reason}
                          </p>
                        </div>
                      </div>
                    );
                  })()}

                <div className="overflow-x-auto rounded-lg border border-[var(--color-border-default)]">
                  <table className="w-full text-left text-xs">
                    <thead>
                      <tr className="border-b border-[var(--color-border-default)] bg-[var(--color-bg-secondary)]">
                        {result.columns.map((col) => (
                          <th
                            key={col.name}
                            className="whitespace-nowrap px-4 py-2.5 font-semibold text-[var(--color-text-secondary)]"
                          >
                            <div>{col.name}</div>
                            <div className="text-[10px] font-normal text-[var(--color-text-disabled)]">
                              {col.data_type}
                            </div>
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {result.rows.slice(0, 500).map((row, ri) => (
                        <tr
                          key={ri}
                          className="border-b border-[var(--color-border-default)] last:border-0 hover:bg-[var(--color-bg-secondary)] transition-colors"
                        >
                          {result.columns.map((col) => (
                            <td
                              key={col.name}
                              className="max-w-xs truncate whitespace-nowrap px-4 py-2 text-[var(--color-text-primary)]"
                              title={cellText(row[col.name])}
                            >
                              {row[col.name] === null ? (
                                <span className="italic text-[var(--color-text-disabled)]">
                                  NULL
                                </span>
                              ) : (
                                cellText(row[col.name])
                              )}
                            </td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {result.row_count > 500 && (
                    <div className="border-t border-[var(--color-border-default)] px-4 py-2 text-[11px] text-[var(--color-text-disabled)] bg-[var(--color-bg-secondary)]">
                      {t('query.truncated', { shown: 500, total: result.row_count })}
                    </div>
                  )}
                </div>
              </div>
            )}

            {!result && !loading && !error && (
              <div className="flex h-full items-center justify-center">
                <div className="text-center">
                  <Database className="mx-auto mb-3 text-[var(--color-text-disabled)]" size={32} />
                  <p className="text-sm text-[var(--color-text-tertiary)]">{t('query.ready')}</p>
                  <p className="mt-1 text-xs text-[var(--color-text-disabled)]">
                    Ctrl+Enter {t('query.toExecute')}
                  </p>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
