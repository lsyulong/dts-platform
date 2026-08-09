import { useCallback, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Sparkles, Send, Play, Copy, Check, Loader2, Database } from 'lucide-react';
import client from '../../api/client';

type QueryResult = { columns: { name: string; data_type: string }[]; rows: Record<string, unknown>[]; row_count: number };

export default function AnalysisPage() {
  const { t } = useTranslation();

  const [question, setQuestion] = useState('');
  const [generatedSql, setGeneratedSql] = useState('');
  const [explanation, setExplanation] = useState('');
  const [result, setResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadingAi, setLoadingAi] = useState(false);
  const [loadingQuery, setLoadingQuery] = useState(false);
  const [copied, setCopied] = useState(false);
  const abortRef = useRef<AbortController | null>(null);

  const askAi = useCallback(async () => {
    const trimmed = question.trim();
    if (!trimmed) return;

    setLoadingAi(true);
    setError(null);
    setGeneratedSql('');
    setExplanation('');
    setResult(null);

    // Build a prompt that asks DeerFlow to generate SQL
    const prompt = `你是一位资深数据分析师。请根据用户的自然语言问题生成 DuckDB SQL 查询。

问题: ${trimmed}

请按以下格式回复:
\`\`\`sql
<你的 SQL 查询>
\`\`\`

说明: <一句话解释这个查询做什么>

要求:
- 使用标准 DuckDB SQL 语法
- 查询 PostgreSQL ATTACH 的表时使用 dts_pg.schema.table 格式
- 如果不确定，请说明并给出最合理的推测
- 只回复上述格式，不要额外内容`;

    try {
      const token = localStorage.getItem('token');
      abortRef.current = new AbortController();

      const response = await fetch(
        `/api/v1/ai/chat?message=${encodeURIComponent(prompt)}`,
        {
          method: 'POST',
          headers: {
            Authorization: `Bearer ${token}`,
            Accept: 'text/event-stream',
          },
          signal: abortRef.current.signal,
        },
      );

      if (!response.ok) throw new Error(`HTTP ${response.status}`);

      const reader = response.body?.getReader();
      if (!reader) throw new Error('No response body');

      const decoder = new TextDecoder();
      let fullText = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const chunk = decoder.decode(value, { stream: true });
        const lines = chunk.split('\n');
        for (const line of lines) {
          if (line.startsWith('data: ')) {
            const data = line.slice(6);
            if (data === '[DONE]') break;
            fullText += data.replace('\\n', '\n');
          }
        }
      }

      // Parse the SQL + explanation from the response
      const sqlMatch = fullText.match(/```sql\s*\n?([\s\S]*?)```/);
      const explainMatch = fullText.match(/说明[:：]\s*(.+)/);

      if (sqlMatch) {
        setGeneratedSql(sqlMatch[1].trim());
      } else {
        // If no code block, treat the whole response as potential SQL
        setGeneratedSql(fullText.trim());
      }
      if (explainMatch) {
        setExplanation(explainMatch[1].trim());
      }
    } catch (err: unknown) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const msg = err instanceof Error ? err.message : 'AI 请求失败';
      setError(msg);
    } finally {
      setLoadingAi(false);
      abortRef.current = null;
    }
  }, [question]);

  const executeSql = useCallback(async () => {
    if (!generatedSql.trim()) return;
    setLoadingQuery(true);
    setError(null);
    setResult(null);
    try {
      const { data } = await client.post<QueryResult>('/query/execute', { sql: generatedSql });
      setResult(data);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : '查询失败';
      setError(msg);
    } finally {
      setLoadingQuery(false);
    }
  }, [generatedSql]);

  const copySql = useCallback(async () => {
    await navigator.clipboard.writeText(generatedSql);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [generatedSql]);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-[var(--color-border-default)] px-6 py-4">
        <div>
          <h1 className="text-lg font-semibold text-[var(--color-text-primary)]">
            <Sparkles className="mr-2 inline-block text-violet-400" size={22} />
            {t('nav.analysis')}
          </h1>
          <p className="mt-0.5 text-xs text-[var(--color-text-tertiary)]">
            DeerFlow AI — NL2SQL
          </p>
        </div>
      </div>

      <div className="flex flex-1 flex-col overflow-y-auto">
        {/* Question Input */}
        <div className="border-b border-[var(--color-border-default)] p-6">
          <div className="mx-auto max-w-3xl">
            <label htmlFor="nl-question" className="mb-2 block text-sm font-medium text-[var(--color-text-secondary)]">
              {t('analysis.askQuestion')}
            </label>
            <div className="flex gap-2">
              <input
                id="nl-question"
                type="text"
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') askAi(); }}
                placeholder={t('analysis.questionPlaceholder')}
                disabled={loadingAi}
                className="flex-1 rounded-xl border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] px-4 py-2.5 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-disabled)] focus:outline-none focus:ring-2 focus:ring-violet-500/30"
              />
              <button
                type="button"
                onClick={askAi}
                disabled={loadingAi || !question.trim()}
                className="inline-flex items-center gap-2 rounded-xl bg-violet-600 px-5 py-2.5 text-sm font-medium text-white shadow-lg shadow-violet-500/25 transition-all hover:bg-violet-500 active:scale-95 disabled:opacity-50"
              >
                {loadingAi ? (
                  <Loader2 size={16} className="animate-spin" />
                ) : (
                  <Send size={16} />
                )}
                {t('analysis.ask')}
              </button>
            </div>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-6">
          <div className="mx-auto max-w-3xl space-y-4">
            {/* AI Loading State */}
            {loadingAi && (
              <div className="flex items-center gap-3 rounded-xl border border-violet-500/20 bg-violet-500/5 p-4">
                <Loader2 size={18} className="animate-spin text-violet-400" />
                <span className="text-sm text-[var(--color-text-tertiary)]">{t('analysis.thinking')}</span>
              </div>
            )}

            {/* Generated SQL */}
            {generatedSql && (
              <div className="rounded-xl border border-[var(--color-border-default)] bg-[var(--color-bg-secondary)]">
                <div className="flex items-center justify-between border-b border-[var(--color-border-default)] px-4 py-3">
                  <span className="text-xs font-medium text-[var(--color-text-secondary)]">
                    {t('analysis.generatedSql')}
                  </span>
                  <div className="flex gap-2">
                    <button
                      type="button"
                      onClick={copySql}
                      className="inline-flex items-center gap-1 rounded-lg px-2.5 py-1 text-[11px] text-[var(--color-text-tertiary)] hover:bg-[var(--color-bg-tertiary)]"
                    >
                      {copied ? <Check size={12} className="text-green-400" /> : <Copy size={12} />}
                      {copied ? t('analysis.copied') : t('analysis.copy')}
                    </button>
                    <button
                      type="button"
                      onClick={executeSql}
                      disabled={loadingQuery}
                      className="inline-flex items-center gap-1 rounded-lg bg-blue-600 px-3 py-1 text-[11px] font-medium text-white hover:bg-blue-500 disabled:opacity-50"
                    >
                      {loadingQuery ? (
                        <Loader2 size={12} className="animate-spin" />
                      ) : (
                        <Play size={12} />
                      )}
                      {t('analysis.execute')}
                    </button>
                  </div>
                </div>
                <pre className="overflow-x-auto p-4 text-[13px] text-[var(--color-text-primary)] font-mono whitespace-pre-wrap">
                  {generatedSql}
                </pre>
                {explanation && (
                  <div className="border-t border-[var(--color-border-default)] px-4 py-2.5">
                    <p className="text-xs text-[var(--color-text-tertiary)]">{explanation}</p>
                  </div>
                )}
              </div>
            )}

            {/* Error */}
            {error && (
              <div className="rounded-xl border border-red-500/20 bg-red-500/5 p-4">
                <p className="text-sm font-medium text-red-400">{t('analysis.error')}</p>
                <pre className="mt-1 text-xs text-red-300/80 whitespace-pre-wrap">{error}</pre>
              </div>
            )}

            {/* Loading Query */}
            {loadingQuery && (
              <div className="flex items-center justify-center py-8">
                <Loader2 size={20} className="animate-spin text-blue-400" />
                <span className="ml-3 text-sm text-[var(--color-text-tertiary)]">{t('analysis.executing')}</span>
              </div>
            )}

            {/* Result Table */}
            {result && !loadingQuery && (
              <div className="rounded-xl border border-[var(--color-border-default)]">
                <div className="flex items-center gap-2 border-b border-[var(--color-border-default)] px-4 py-3">
                  <Database size={14} className="text-blue-400" />
                  <span className="text-xs text-[var(--color-text-tertiary)]">
                    {t('analysis.rowsReturned', { count: result.row_count })}
                  </span>
                </div>
                <div className="overflow-x-auto">
                  <table className="w-full text-left text-xs">
                    <thead>
                      <tr className="border-b border-[var(--color-border-default)] bg-[var(--color-bg-secondary)]">
                        {result.columns.map((col) => (
                          <th key={col.name} className="whitespace-nowrap px-4 py-2.5 font-semibold text-[var(--color-text-secondary)]">
                            {col.name}
                            <span className="ml-1 text-[10px] font-normal text-[var(--color-text-disabled)]">
                              ({col.data_type})
                            </span>
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {result.rows.slice(0, 100).map((row, ri) => (
                        <tr
                          key={ri}
                          className="border-b border-[var(--color-border-default)] last:border-0 hover:bg-[var(--color-bg-secondary)]"
                        >
                          {result.columns.map((col) => (
                            <td key={col.name} className="max-w-[200px] truncate whitespace-nowrap px-4 py-2 text-[var(--color-text-primary)]">
                              {row[col.name] === null
                                ? <span className="italic text-[var(--color-text-disabled)]">NULL</span>
                                : String(row[col.name])
                              }
                            </td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}

            {/* Empty State */}
            {!generatedSql && !loadingAi && !error && (
              <div className="flex flex-col items-center justify-center py-20">
                <Sparkles className="mb-4 text-[var(--color-text-disabled)]" size={40} />
                <p className="text-sm text-[var(--color-text-tertiary)]">{t('analysis.ready')}</p>
                  <p className="mt-2 text-xs text-[var(--color-text-disabled)] max-w-sm text-center">
                    {t('analysis.hint')}
                  </p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
