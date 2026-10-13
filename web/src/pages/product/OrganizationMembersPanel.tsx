import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import {
  useOrganizationDetails,
  useOrganizationMembers,
  useOrganizationSession,
  useUserMembership,
} from '../../product/organization-members/api';
import type {
  OrganizationDetails,
  OrganizationIdentity,
  OrganizationMember,
} from '../../product/organization-members/types';
import { useOrganizationOperations } from '../../product/organization-members/operations';
import OrganizationOperationDialog from '../../product/organization-members/Dialog';
import OrganizationReferencePicker from '../../product/organization-members/Picker';
import OrganizationMemberTable from '../../product/organization-members/MemberTable';
import OrganizationHistory from '../../product/organization-members/History';

export interface OrganizationMembersPanelProps {
  organizationId: string;
}
export default function OrganizationMembersPanel({
  organizationId,
}: OrganizationMembersPanelProps) {
  const session = useOrganizationSession();
  return <Members key={`${session.key}:${organizationId}`} organizationId={organizationId} />;
}
const identity = (details: OrganizationDetails): OrganizationIdentity => ({
  id: details.organization.id,
  code: details.organization.code,
  name: details.organization.name,
  version: details.organization.version,
  parent_id: details.organization.parent_id,
});
function Members({ organizationId }: OrganizationMembersPanelProps) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const session = useOrganizationSession(),
    operations = useOrganizationOperations(session);
  const [q, setQ] = useState(''),
    [offset, setOffset] = useState(0),
    [userId, setUserId] = useState('');
  const details = useOrganizationDetails(organizationId),
    members = useOrganizationMembers(organizationId, q, offset),
    user = useUserMembership(userId);
  const selected = user.isError || user.isFetching ? null : user.data;
  const originId = selected?.membership?.organization_id ?? '';
  const origin = useOrganizationDetails(originId);
  const current = details.isError || details.isFetching ? null : details.data;
  const page = members.isError || members.isFetching ? null : (members.data ?? null);
  const source = !origin.isError && !origin.isFetching ? origin.data : null;
  const canAdd =
    !!current &&
    !!selected &&
    selected.user.status === 'active' &&
    originId !== organizationId &&
    (!originId || !!source);
  const beginMember = (member: OrganizationMember, kind: 'transfer' | 'remove' | 'manager') => {
    if (!current) return;
    const common = {
      source: identity(current),
      userId: member.user.id,
      userName: member.user.display_name || member.user.username,
      isManager: member.is_manager,
      reason: '',
      handoverChoice: '' as const,
      successorId: '',
    };
    if (kind === 'manager') operations.begin({ ...common, kind, managerUserId: member.user.id });
    else if (kind === 'remove')
      operations.begin({ ...common, kind, membershipVersion: member.membership.version });
    else
      operations.begin({
        ...common,
        kind,
        membershipVersion: member.membership.version,
        target: null,
        targetId: '',
      });
  };
  if (!session.ready || !session.canReadDefinitions)
    return (
      <ErrorBanner
        message={text(
          '需要当前有效身份和组织查看权限',
          'An active identity and organization read permission are required',
        )}
      />
    );
  if (!organizationId)
    return (
      <p className="data-panel p-5">{text('请选择实际组织', 'Choose an actual organization')}</p>
    );
  return (
    <div className="space-y-5">
      <section className="data-panel p-5">
        <div className="flex items-center justify-between gap-3">
          <h2 className="font-medium">
            {text('组织成员与负责人', 'Organization members and manager')}
          </h2>
          <Button
            variant="outline"
            disabled={details.isFetching || members.isFetching || operations.busy}
            onClick={() => {
              void details.refetch();
              void members.refetch();
              if (userId) void user.refetch();
              if (originId) void origin.refetch();
            }}
          >
            {text('刷新组织信息', 'Refresh organization')}
          </Button>
        </div>
        {details.error ? (
          <ErrorBanner
            message={apiErrorMessage(
              details.error,
              text('组织加载失败', 'Organization failed to load'),
            )}
          />
        ) : !current ? (
          <p className="mt-4" role="status">
            {text('正在读取实际组织…', 'Loading the organization…')}
          </p>
        ) : (
          <div className="mt-4 space-y-2 text-sm">
            <p>
              {current.organization.name} · v{current.organization.version} ·{' '}
              {text('成员总数', 'Members')} {current.member_count} ·{' '}
              {text('直接子组织', 'Direct child organizations')} {current.children_count}
            </p>
            <p>
              {text('当前负责人', 'Current manager')}:{' '}
              {current.manager
                ? current.manager.user.display_name || current.manager.user.username
                : text('未设置', 'Unassigned')}
              {current.manager &&
                !current.manager_valid &&
                ` · ${text('账号已停用，需要调整', 'Inactive account; reassignment required')}`}
            </p>
            {current.legacy_membership_ignored && (
              <p className="text-[var(--color-text-secondary)]">
                {text(
                  '旧配置中的成员或负责人字段尚待核对，不作为实际归属。请通过下方流程重新分配。',
                  'Legacy member/manager settings require reconciliation and are not current membership. Use the assignment workflow below.',
                )}
              </p>
            )}
            {session.canManage && current.manager && (
              <Button
                size="sm"
                variant="outline"
                disabled={operations.busy}
                onClick={() =>
                  operations.begin({
                    kind: 'manager',
                    source: identity(current),
                    userId: current.manager?.user.id ?? null,
                    userName: current.manager?.user.username ?? '',
                    isManager: true,
                    reason: '',
                    managerUserId: null,
                    handoverChoice: '',
                    successorId: '',
                  })
                }
              >
                {text('负责人卸任', 'Remove manager')}
              </Button>
            )}
          </div>
        )}
        <p className="mt-4 text-xs text-[var(--color-text-tertiary)]">
          {text(
            '同一账号当前只能归属一个组织。调动不会新增或撤销角色权限；现任负责人离开前需完成继任或明确卸任。',
            'Each account has one current organization. Transfers do not change roles; manager departure requires succession or explicit resignation.',
          )}
        </p>
      </section>
      {session.canManage && (
        <section className="data-panel p-5">
          <div className="grid items-end gap-4 sm:grid-cols-2">
            <OrganizationReferencePicker
              kind="user"
              label={text('加入或调入成员', 'Member to assign or transfer')}
              value={userId}
              onChange={setUserId}
              disabled={operations.busy}
            />
            <div className="space-y-3">
              <p className="text-sm">
                {selected
                  ? `${text('成员版本', 'Membership version')}: ${selected.membership?.version ?? 0} · ${source?.organization.name ?? (originId ? text('正在核对原组织', 'Checking original organization') : text('未归属', 'Unassigned'))}`
                  : text('请选择并核对实际用户归属', 'Select a user and verify current membership')}
              </p>
              <Button
                disabled={!canAdd || operations.busy}
                onClick={() => {
                  if (current && selected)
                    operations.begin({
                      kind: 'transfer',
                      source: source ? identity(source) : null,
                      target: identity(current),
                      targetId: current.organization.id,
                      userId: selected.user.id,
                      userName: selected.user.display_name || selected.user.username,
                      isManager: source?.manager?.user.id === selected.user.id,
                      reason: '',
                      membershipVersion: selected.membership?.version ?? 0,
                      handoverChoice: '',
                      successorId: '',
                    });
                }}
              >
                {text('加入 / 调入当前组织', 'Assign / transfer to this organization')}
              </Button>
            </div>
          </div>
          {(user.error || origin.error) && (
            <div className="mt-3">
              <ErrorBanner
                message={apiErrorMessage(
                  user.error ?? origin.error,
                  text('成员归属加载失败', 'Could not load current membership'),
                )}
              />
            </div>
          )}
        </section>
      )}
      <div className="data-panel p-4">
        <label htmlFor="organization-member-search" className="mr-3 text-sm">
          {text('查找实际成员', 'Search actual members')}
        </label>
        <input
          id="organization-member-search"
          value={q}
          maxLength={128}
          disabled={operations.busy}
          onChange={(event) => {
            setQ(event.target.value);
            setOffset(0);
          }}
          className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
        />
      </div>
      {members.error && (
        <ErrorBanner
          message={apiErrorMessage(
            members.error,
            text('成员列表加载失败', 'Member list failed to load'),
          )}
        />
      )}
      <OrganizationMemberTable
        page={page}
        loading={members.isFetching}
        offset={offset}
        busy={operations.busy || !current}
        canManage={session.canManage && !!current}
        onPage={setOffset}
        onTransfer={(member) => beginMember(member, 'transfer')}
        onRemove={(member) => beginMember(member, 'remove')}
        onManager={(member) => beginMember(member, 'manager')}
      />
      {operations.result && (
        <p className="data-panel p-4 text-sm" role="status">
          {operations.result.replayed
            ? text('已取得原操作回执', 'Original operation receipt replayed')
            : text('组织变更已提交', 'Organization change committed')}{' '}
          · {text('回执', 'Receipt')}: {operations.result.receipt_id} ·{' '}
          {text('角色权限未变更', 'Role permissions unchanged')}
        </p>
      )}
      <OrganizationHistory organizationId={organizationId} />
      <OrganizationOperationDialog operations={operations} />
    </div>
  );
}
