import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import Editor, { type OnMount } from '@monaco-editor/react';
import { Play, Database, Clock, ChevronDown, Trash2, Loader2 } from 'lucide-react';
import client from '../../api/client';

type ColumnMeta = { name: string; data_type: string };
type QueryResult = { columns: ColumnMeta[]; rows: Record<string, unknown>[]; row_count: number };
type SchemaInfo = { catalog: string | null; schema: string; tables: { name: string }[] };

const HISTORY_KEY = 'dts-query-history';
const MAX_HISTORY = 50;

interface HistoryEntry {
  sql: string;
  timestamp: number;
}

export default function QueryPage() {
  const { t } = useTranslation();

  const [sql, setSql] = useState('SELECT 1 AS hello');
  const [result, setResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [schemas, setSchemas] = useState<SchemaInfo[]>([]);
  const [schemaOpen, setSchemaOpen] = useState(false);
  const [history, setHistory] = useState<HistoryEntry[]>(() => {
    try {
      return JSON.parse(localStorage.getItem(HISTORY_KEY) || '[]');
    } catch {
      return [];
    }
  });
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);

  // Persist history
  useEffect(() => {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history.slice(0, MAX_HISTORY)));
  }, [history]);

  // Ctrl+Enter to execute
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        executeQuery();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  });

  const executeQuery = useCallback(async () => {
    const trimmed = sql.trim();
    if (!trimmed) return;
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const { data } = await client.post<QueryResult>('/query/execute', { sql: trimmed });
      setResult(data);
      setHistory((prev) => [{ sql: trimmed, timestamp: Date.now() }, ...prev]);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : '查询失败';
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [sql]);

  const fetchSchemas = useCallback(async () => {
    try {
      const { data } = await client.get<SchemaInfo[]>('/query/schemas');
      setSchemas(data);
    } catch {
      // schemas fetch is best-effort
    }
  }, []);

  const handleEditorMount: OnMount = useCallback((editor) => {
    editorRef.current = editor;
  }, []);

  const loadHistoryEntry = useCallback((entry: HistoryEntry) => {
    setSql(entry.sql);
  }, []);

  const clearHistory = useCallback(() => {
    setHistory([]);
    localStorage.removeItem(HISTORY_KEY);
  }, []);

  const insertTableName = useCallback((schema: string, table: string) => {
    setSql((prev) => prev + `"${schema}"."${table}"`);
    // Focus the editor
    editorRef.current?.focus();
  }, []);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-[var(--color-border-default)] px-6 py-4">
        <div>
          <h1 className="text-lg font-semibold text-[var(--color-text-primary)]">
            <Database className="mr-2 inline-block text-blue-400" size={22} />
            {t('nav.query')}
          </h1>
          <p className="mt-0.5 text-xs text-[var(--color-text-tertiary)]">
            SwanLake DuckDB — Arrow Flight SQL
          </p>
        </div>
        <button
          type="button"
          onClick={executeQuery}
          disabled={loading || !sql.trim()}
          className="inline-flex items-center gap-2 rounded-xl bg-blue-600 px-5 py-2.5 text-sm font-medium text-white shadow-lg shadow-blue-500/25 transition-all hover:bg-blue-500 active:scale-95 disabled:opacity-50"
        >
          {loading ? (
            <Loader2 size={16} className="animate-spin" />
          ) : (
            <Play size={16} />
          )}
          {t('query.execute')}
        </button>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* Schema Explorer Sidebar */}
        <aside className="w-56 shrink-0 overflow-y-auto border-r border-[var(--color-border-default)] bg-[var(--color-bg-secondary)] p-3">
          <button
            type="button"
            onClick={() => { setSchemaOpen(!schemaOpen); if (!schemaOpen) fetchSchemas(); }}
            className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-xs font-medium text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)]"
          >
            <span className="flex items-center gap-2">
              <Database size={14} />
              {t('query.schemas')}
            </span>
            <ChevronDown size={14} className={`transition-transform ${schemaOpen ? 'rotate-180' : ''}`} />
          </button>

          {schemaOpen && schemas.length === 0 && (
            <p className="mt-2 px-3 text-[11px] text-[var(--color-text-disabled)]">{t('query.noSchemas')}</p>
          )}

          {schemaOpen && schemas.map((s) => (
            <div key={s.schema} className="mt-2">
              <p className="px-3 text-[11px] font-semibold uppercase text-[var(--color-text-disabled)]">
                {s.schema}
              </p>
              {s.tables.map((tbl) => (
                <button
                  key={`${s.schema}.${tbl.name}`}
                  type="button"
                  onClick={() => insertTableName(s.schema, tbl.name)}
                  className="flex w-full items-center gap-2 rounded-md px-5 py-1 text-left text-[11px] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]"
                >
                  {tbl.name}
                </button>
              ))}
            </div>
          ))}

          {/* Query History */}
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
                  className="rounded p-0.5 text-[var(--color-text-disabled)] hover:text-red-400"
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
                  {entry.sql.slice(0, 60)}{entry.sql.length > 60 ? '…' : ''}
                </button>
              ))}
            </div>
          </div>
        </aside>

        {/* Main Editor + Results */}
        <div className="flex flex-1 flex-col overflow-hidden">
          {/* SQL Editor */}
          <div className="h-48 shrink-0 border-b border-[var(--color-border-default)]">
            <Editor
              height="100%"
              defaultLanguage="sql"
              value={sql}
              onChange={(v) => setSql(v || '')}
              onMount={handleEditorMount}
              theme="vs-dark"
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

          {/* Result Area */}
          <div className="flex-1 overflow-auto">
            {error && (
              <div className="m-4 rounded-xl border border-red-500/20 bg-red-500/5 p-4">
                <p className="text-sm font-medium text-red-400">{t('query.error')}</p>
                <pre className="mt-1 text-xs text-red-300/80 whitespace-pre-wrap">{error}</pre>
              </div>
            )}

            {loading && (
              <div className="flex items-center justify-center py-20">
                <Loader2 size={24} className="animate-spin text-blue-400" />
                <span className="ml-3 text-sm text-[var(--color-text-tertiary)]">{t('query.executing')}</span>
              </div>
            )}

            {result && !loading && (
              <div className="p-4">
                <p className="mb-3 text-xs text-[var(--color-text-tertiary)]">
                  {t('query.rowsReturned', { count: result.row_count })}
                  {result.columns.length > 0 && ` · ${result.columns.length} ${t('query.columns')}`}
                </p>
                <div className="overflow-x-auto rounded-xl border border-[var(--color-border-default)]">
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
                              title={String(row[col.name] ?? 'NULL')}
                            >
                              {row[col.name] === null ? (
                                <span className="italic text-[var(--color-text-disabled)]">NULL</span>
                              ) : (
                                String(row[col.name])
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
                  <p className="mt-1 text-xs text-[var(--color-text-disabled)]">Ctrl+Enter {t('query.toExecute')}</p>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
