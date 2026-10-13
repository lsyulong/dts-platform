import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { captureSession, isCurrentSession } from '../../lib/session';
import { apiErrorMessage } from '../../lib/apiError';

export default function JobResultExports({
  jobId,
  nodeId,
  sourceVersion,
}: {
  jobId: string;
  nodeId: string;
  sourceVersion: number;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState('');
  const download = async (format: 'json' | 'csv') => {
    if (busy) return;
    const session = captureSession();
    setBusy(true);
    setError('');
    try {
      const response = await client.post<Blob>(
        `/product/jobs/${encodeURIComponent(jobId)}/export`,
        { node_id: nodeId, expected_source_version: sourceVersion, format },
        { headers: { Authorization: `Bearer ${session.token}` }, responseType: 'blob' },
      );
      if (!isCurrentSession(session)) return;
      const url = URL.createObjectURL(response.data);
      const link = document.createElement('a');
      link.href = url;
      link.download = `query-result-${jobId}.${format}`;
      link.click();
      URL.revokeObjectURL(url);
    } catch (caught) {
      if (isCurrentSession(session))
        setError(
          apiErrorMessage(
            caught,
            text('结果导出失败，请检查当前授权', 'Export failed; check current access'),
          ),
        );
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  return (
    <div className="mt-3 space-y-2">
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" size="sm" disabled={busy} onClick={() => void download('json')}>
          {text('导出 JSON', 'Export JSON')}
        </Button>
        <Button variant="outline" size="sm" disabled={busy} onClick={() => void download('csv')}>
          {text('导出 CSV', 'Export CSV')}
        </Button>
      </div>
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text(
          '导出会核对当前数据权限与脱敏规则。JSON 保留完整值；CSV 使用文本转义保护精度，以 \\N 表示空值。',
          'Exports check current data access and masking. JSON preserves values; CSV uses text escaping to protect precision and \\N for null.',
        )}
      </p>
      {error && <ErrorBanner message={error} />}
    </div>
  );
}
