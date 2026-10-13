import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import { useAuthStore } from '../../stores/auth';
import { statusText } from '../../product/ui-text';
import { useDataAccessPage, type AccessAuditPage } from './data-access-api';

export default function DataAccessAuditPage() {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <Audit key={identity} />;
}

function Audit() {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const canRead = !!useAuthStore((state) => state.user?.permissions.includes('data_security:read'));
  const [offset, setOffset] = useState(0);
  const page = useDataAccessPage<AccessAuditPage>('audit', '', offset, canRead);
  const download = () => {
    if (!canRead || !page.data) return;
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(page.data.data, null, 2)], { type: 'application/json' }),
    );
    const link = document.createElement('a');
    link.href = url;
    link.download = 'data-access-audit.json';
    link.click();
    URL.revokeObjectURL(url);
  };
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold">{text('数据权限审计', 'Data access audit')}</h1>
          <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
            {text(
              '查看实际申请处理与查询访问事件，核对授权、字段和访问结果',
              'Inspect actual decisions and query access events with grant and column evidence',
            )}
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            disabled={!canRead || !page.data?.data.length}
            onClick={download}
          >
            {text('导出当前页', 'Export current page')}
          </Button>
          <Button
            variant="outline"
            disabled={!canRead || page.isFetching}
            onClick={() => void page.refetch()}
          >
            {text('刷新', 'Refresh')}
          </Button>
        </div>
      </div>
      {!canRead && (
        <ErrorBanner
          message={text('当前账号没有查看权限', 'Your account cannot read audit events')}
        />
      )}
      {page.error && (
        <ErrorBanner
          message={apiErrorMessage(
            page.error,
            text('审计记录加载失败', 'Could not load audit events'),
          )}
        />
      )}
      <section className="data-panel">
        <div className="overflow-auto">
          <table className="w-full text-left text-sm">
            <thead className="bg-[var(--color-bg-secondary)]">
              <tr>
                {[
                  text('时间', 'Time'),
                  text('访问者', 'Actor'),
                  text('动作', 'Action'),
                  text('结果', 'Outcome'),
                  text('审计证据', 'Evidence'),
                ].map((label) => (
                  <th key={label} className="p-4 font-medium">
                    {label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {canRead &&
                page.data?.data.map((event) => (
                  <tr key={event.id} className="border-t border-[var(--color-border-default)]">
                    <td className="whitespace-nowrap p-4">
                      {new Date(event.created_at).toLocaleString(i18n.language)}
                    </td>
                    <td className="p-4 text-xs">{event.actor_id}</td>
                    <td className="p-4">{statusText(event.action, i18n.language)}</td>
                    <td className="p-4">{statusText(event.outcome, i18n.language)}</td>
                    <td className="p-4">
                      <details>
                        <summary className="cursor-pointer text-[var(--color-accent)]">
                          {text('查看证据', 'Inspect evidence')}
                        </summary>
                        <pre className="mt-2 max-h-64 max-w-lg overflow-auto whitespace-pre-wrap break-words rounded bg-[var(--color-bg-secondary)] p-3 text-xs">
                          {JSON.stringify(event.evidence, null, 2)}
                        </pre>
                      </details>
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
        {canRead && !page.data?.data.length && (
          <p className="py-14 text-center text-sm text-[var(--color-text-tertiary)]">
            {page.isPending
              ? text('正在加载', 'Loading')
              : text('没有实际访问审计记录', 'No persisted access audit events')}
          </p>
        )}
        <div className="flex justify-between border-t border-[var(--color-border-default)] p-4 text-sm">
          <span>
            {canRead && page.data ? `${page.data.total} ${text('条事件', 'events')}` : '—'}
          </span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={!canRead || !offset || page.isFetching}
              onClick={() => setOffset(Math.max(0, offset - 20))}
            >
              {text('上一页', 'Previous')}
            </Button>
            <Button
              variant="outline"
              disabled={!canRead || !page.data || offset + 20 >= page.data.total || page.isFetching}
              onClick={() => setOffset(offset + 20)}
            >
              {text('下一页', 'Next')}
            </Button>
          </div>
        </div>
      </section>
    </div>
  );
}
