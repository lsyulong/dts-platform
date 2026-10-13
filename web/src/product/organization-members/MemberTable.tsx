import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import type { OrganizationMember, OrganizationMemberPage } from './types';

export default function OrganizationMemberTable({
  page,
  loading,
  offset,
  busy,
  canManage,
  onPage,
  onTransfer,
  onRemove,
  onManager,
}: {
  page: OrganizationMemberPage | null;
  loading: boolean;
  offset: number;
  busy: boolean;
  canManage: boolean;
  onPage: (offset: number) => void;
  onTransfer: (member: OrganizationMember) => void;
  onRemove: (member: OrganizationMember) => void;
  onManager: (member: OrganizationMember) => void;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  return (
    <section className="data-panel">
      <div className="data-toolbar">
        <h2 className="font-medium">{text('实际组织成员', 'Actual organization members')}</h2>
      </div>
      {page?.data.length ? (
        <div className="overflow-auto">
          <table className="data-table">
            <thead>
              <tr>
                {[
                  text('成员', 'Member'),
                  text('账号状态', 'Account status'),
                  text('成员版本', 'Membership version'),
                  text('最近变更', 'Last change'),
                  text('操作', 'Actions'),
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {page.data.map((member) => (
                <tr key={member.membership.id}>
                  <td>
                    {member.user.display_name || member.user.username}
                    {member.is_manager && (
                      <span className="ml-2 text-xs text-[var(--color-action)]">
                        {text('负责人', 'Manager')}
                      </span>
                    )}
                  </td>
                  <td>{member.user.status}</td>
                  <td>{member.membership.version}</td>
                  <td>{new Date(member.membership.changed_at).toLocaleString(i18n.language)}</td>
                  <td>
                    {canManage && (
                      <div className="flex flex-wrap gap-2">
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={busy || member.user.status !== 'active'}
                          onClick={() => onTransfer(member)}
                        >
                          {text('调动', 'Transfer')}
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={busy}
                          onClick={() => onRemove(member)}
                        >
                          {text('移除', 'Remove')}
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={busy || member.user.status !== 'active' || member.is_manager}
                          onClick={() => onManager(member)}
                        >
                          {text('设为负责人', 'Set manager')}
                        </Button>
                      </div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p role="status" className="p-5 text-sm">
          {loading
            ? text('正在加载成员…', 'Loading members…')
            : page
              ? text('当前筛选下没有实际成员', 'No actual members match this filter')
              : text('尚未取得成员列表', 'Member list is unavailable')}
        </p>
      )}
      <div className="flex justify-between p-4 text-sm">
        <span>
          {text('当前筛选共', 'Filtered total')} {page?.total ?? '—'}
        </span>
        <div className="flex gap-2">
          <Button
            variant="outline"
            disabled={!offset || loading || busy}
            onClick={() => onPage(Math.max(0, offset - 20))}
          >
            {text('成员上一页', 'Previous members')}
          </Button>
          <Button
            variant="outline"
            disabled={!page || offset + 20 >= page.total || loading || busy}
            onClick={() => onPage(offset + 20)}
          >
            {text('成员下一页', 'Next members')}
          </Button>
        </div>
      </div>
    </section>
  );
}
