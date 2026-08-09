import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { FileText, Loader2 } from 'lucide-react';

type Props = {
  taskId: string;
};

export function AiReportPanel({ taskId }: Props) {
  const { t } = useTranslation();
  const [report, setReport] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [generated, setGenerated] = useState(false);

  const generate = useCallback(async () => {
    setLoading(true);
    setError(null);
    setReport('');
    setGenerated(false);

    try {
      const token = localStorage.getItem('token');
      const response = await fetch(
        `/api/v1/ai/report?task_id=${encodeURIComponent(taskId)}`,
        {
          method: 'POST',
          headers: {
            Authorization: `Bearer ${token}`,
            Accept: 'text/event-stream',
          },
        },
      );

      if (!response.ok) throw new Error(`HTTP ${response.status}`);

      const reader = response.body?.getReader();
      if (!reader) throw new Error('No response body');

      const decoder = new TextDecoder();
      let content = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const chunk = decoder.decode(value, { stream: true });
        const lines = chunk.split('\n');
        for (const line of lines) {
          if (line.startsWith('data: ')) {
            const data = line.slice(6);
            if (data === '[DONE]') break;
            content += data.replace('\\n', '\n');
            setReport(content);
          }
        }
      }
      setGenerated(true);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : '请求失败';
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [taskId]);

  return (
    <div className="space-y-4">
      {!generated && !loading && (
        <button
          type="button"
          onClick={generate}
          className="inline-flex items-center gap-2 rounded-xl bg-violet-600 px-4 py-2.5 text-sm font-medium text-white shadow-lg shadow-violet-500/25 transition-all hover:bg-violet-500 active:scale-95"
        >
          <FileText size={16} />
          生成 AI 分析报告
        </button>
      )}

      {loading && (
        <div className="flex items-center gap-3 rounded-xl border border-violet-500/20 bg-violet-500/5 p-4">
          <Loader2 size={18} className="animate-spin text-violet-400" />
          <span className="text-sm text-[var(--color-text-tertiary)]">DeerFlow AI 正在生成分析报告...</span>
        </div>
      )}

      {error && (
        <div className="rounded-xl border border-red-500/20 bg-red-500/5 p-4">
          <p className="text-sm font-medium text-red-400">生成失败</p>
          <p className="mt-1 text-xs text-red-300/80">{error}</p>
        </div>
      )}

      {report && (
        <div className="rounded-xl border border-[var(--color-border-default)] bg-[var(--color-bg-primary)]">
          <div className="border-b border-[var(--color-border-default)] px-5 py-3">
            <h3 className="text-sm font-semibold text-[var(--color-text-primary)] flex items-center gap-2">
              <FileText size={16} className="text-violet-400" />
              AI 分析报告
            </h3>
          </div>
          <div className="p-5 prose prose-sm max-w-none dark:prose-invert">
            <MarkdownRenderer content={report} />
          </div>
          {generated && (
            <div className="border-t border-[var(--color-border-default)] px-5 py-3 flex gap-2">
              <button
                type="button"
                onClick={generate}
                disabled={loading}
                className="text-xs text-violet-400 hover:text-violet-300 font-medium"
              >
                🔄 重新生成
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** Simple Markdown → HTML renderer */
function MarkdownRenderer({ content }: { content: string }) {
  // Convert basic Markdown to HTML
  const html = content
    // Headers
    .replace(/^### (.+)$/gm, '<h4 class="text-sm font-semibold mt-4 mb-1 text-[var(--color-text-primary)]">$1</h4>')
    .replace(/^## (.+)$/gm, '<h3 class="text-base font-bold mt-5 mb-2 text-[var(--color-text-primary)]">$1</h3>')
    // Bold
    .replace(/\*\*(.+?)\*\*/g, '<strong class="font-semibold">$1</strong>')
    // Italic
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    // Code blocks
    .replace(/```([\s\S]*?)```/g, '<pre class="bg-[var(--color-bg-secondary)] rounded-lg p-3 my-2 overflow-x-auto text-xs"><code>$1</code></pre>')
    // Inline code
    .replace(/`(.+?)`/g, '<code class="bg-[var(--color-bg-tertiary)] px-1.5 py-0.5 rounded text-xs">$1</code>')
    // Tables (basic)
    .replace(/^\|(.+)\|$/gm, (match) => {
      const cells = match.split('|').filter(c => c.trim());
      if (cells.every(c => /^[-:]+$/.test(c.trim()))) return ''; // separator row
      const td = cells.map(c => `<td class="px-3 py-1.5 border border-[var(--color-border-default)] text-xs">${c.trim()}</td>`).join('');
      return `<tr>${td}</tr>`;
    })
    // Lists
    .replace(/^- (.+)$/gm, '<li class="ml-4 list-disc text-xs text-[var(--color-text-secondary)]">$1</li>')
    // Paragraphs (double newlines)
    .replace(/\n\n/g, '</p><p class="text-xs leading-relaxed text-[var(--color-text-secondary)]">');

  return (
    <div
      className="space-y-1 text-xs leading-relaxed text-[var(--color-text-secondary)]"
      dangerouslySetInnerHTML={{
        __html: `<p class="text-xs">${html}</p>`,
      }}
    />
  );
}
