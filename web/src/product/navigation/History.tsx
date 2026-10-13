import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import { useNavigationQuery } from './api';
import type { NavigationSession } from './session';
import type { NavigationHistoryPage } from './types';

export default function NavigationHistory({ session }: { session: NavigationSession }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [offset, setOffset] = useState(0);
  const query = useNavigationQuery<NavigationHistoryPage>(
    session,
    '/product/navigation/history',
    { limit: 20, offset },
    session.canManage,
  );
  const page = query.isError || query.isFetching ? null : query.data;
  if (!session.canManage) return null;
  return (
    <section className="data-panel">
      <div className="data-toolbar">
        <h2 className="font-medium">
          {text('分配与撤销历史', 'Assignment and revocation history')}
        </h2>
      </div>
      {query.isError ? (
        <div className="p-4">
          <ErrorBanner
            message={apiErrorMessage(query.error, text('历史加载失败', 'Could not load history'))}
          />
        </div>
      ) : !page ? (
        <p className="p-5" role="status">
          {text('正在加载历史…', 'Loading history…')}
        </p>
      ) : !page.data.length ? (
        <p className="p-5">{text('暂无导航操作记录', 'No navigation operations recorded')}</p>
      ) : (
        <div className="overflow-auto">
          <table className="data-table">
            <thead>
              <tr>
                {[
                  text('时间', 'Time'),
                  text('动作', 'Action'),
                  text('模板快照', 'Template snapshot'),
                  text('策略版本', 'Assignment version'),
                  text('操作人', 'Actor'),
                  text('回执 ID', 'Receipt ID'),
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {page.data.map((entry) => (
                <tr key={entry.id}>
                  <td>{new Date(entry.created_at).toLocaleString(i18n.language)}</td>
                  <td>
                    {entry.action === 'assign' ? text('分配', 'Assign') : text('撤销', 'Revoke')}
                  </td>
                  <td>
                    {entry.result.template.name} · v{entry.result.template.version} ·{' '}
                    {entry.result.template.menus.length} {text('个菜单', 'menus')}
                  </td>
                  <td>{entry.assignment_version}</td>
                  <td className="break-all text-xs">{entry.actor_id}</td>
                  <td className="break-all text-xs">{entry.id}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="flex items-center justify-between p-4 text-sm">
        <span>
          {text('共', 'Total')} {page?.total ?? '—'} {text('条', 'records')}
        </span>
        <div className="flex gap-2">
          <Button
            variant="outline"
            disabled={!offset || query.isFetching}
            onClick={() => setOffset(Math.max(0, offset - 20))}
          >
            {text('上一页', 'Previous')}
          </Button>
          <Button
            variant="outline"
            disabled={!page || offset + 20 >= page.total || query.isFetching}
            onClick={() => setOffset(offset + 20)}
          >
            {text('下一页', 'Next')}
          </Button>
        </div>
      </div>
    </section>
  );
}
