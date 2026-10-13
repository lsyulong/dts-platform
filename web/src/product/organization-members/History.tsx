import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import { useOrganizationQuery } from './api';
import type { OrganizationHistoryPage } from './types';

export default function OrganizationHistory({ organizationId }: { organizationId: string }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [offset, setOffset] = useState(0);
  const query = useOrganizationQuery<OrganizationHistoryPage>(
    '/product/organization-members/history',
    { organization_id: organizationId, limit: 20, offset },
    !!organizationId,
  );
  const page = query.isError || query.isFetching ? null : query.data;
  const actions = {
    transfer: text('加入/调动', 'Assign/transfer'),
    remove: text('移除', 'Remove'),
    set_manager: text('负责人任免', 'Manager change'),
  };
  return (
    <section className="data-panel">
      <div className="data-toolbar">
        <h2 className="font-medium">
          {text('成员与负责人变更历史', 'Membership and manager history')}
        </h2>
      </div>
      {query.error ? (
        <div className="p-4">
          <ErrorBanner
            message={apiErrorMessage(query.error, text('历史加载失败', 'History failed to load'))}
          />
        </div>
      ) : !page ? (
        <p className="p-5" role="status">
          {text('正在加载历史…', 'Loading history…')}
        </p>
      ) : !page.data.length ? (
        <p className="p-5">
          {text('暂无实际组织变更记录', 'No actual organization changes recorded')}
        </p>
      ) : (
        <div className="overflow-auto">
          <table className="data-table">
            <thead>
              <tr>
                {[
                  text('时间', 'Time'),
                  text('动作', 'Action'),
                  text('原/目标组织快照', 'Original/target snapshot'),
                  text('原因', 'Reason'),
                  text('操作人', 'Actor'),
                  text('回执', 'Receipt'),
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {page.data.map((entry) => (
                <tr key={entry.id}>
                  <td>{new Date(entry.created_at).toLocaleString(i18n.language)}</td>
                  <td>{actions[entry.action]}</td>
                  <td>
                    {entry.result.source?.name ?? text('未归属', 'Unassigned')} →{' '}
                    {entry.result.target?.name ?? text('无归属', 'Unassigned')}
                  </td>
                  <td className="max-w-72 whitespace-pre-wrap break-words">{entry.reason}</td>
                  <td className="break-all text-xs">{entry.actor_id}</td>
                  <td className="break-all text-xs">{entry.id}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="flex justify-between p-4 text-sm">
        <span>
          {text('共', 'Total')} {page?.total ?? '—'} {text('条', 'records')}
        </span>
        <div className="flex gap-2">
          <Button
            variant="outline"
            disabled={!offset || query.isFetching}
            onClick={() => setOffset(Math.max(0, offset - 20))}
          >
            {text('历史上一页', 'Previous history')}
          </Button>
          <Button
            variant="outline"
            disabled={!page || offset + 20 >= page.total || query.isFetching}
            onClick={() => setOffset(offset + 20)}
          >
            {text('历史下一页', 'Next history')}
          </Button>
        </div>
      </div>
    </section>
  );
}
