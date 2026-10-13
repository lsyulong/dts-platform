import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { Dialog, DialogContent, DialogTitle, DialogDescription } from '../../components/ui/dialog';
import { apiErrorMessage } from '../../lib/apiError';
import { useOrganizationDetails, useOrganizationMembers } from './api';
import { type PendingOrganizationOperation, type useOrganizationOperations } from './operations';
import OrganizationReferencePicker from './Picker';

function Successor({
  pending,
  change,
  disabled,
}: {
  pending: PendingOrganizationOperation;
  change: (id: string) => void;
  disabled: boolean;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [q, setQ] = useState(''),
    [offset, setOffset] = useState(0);
  const query = useOrganizationMembers(pending.source?.id ?? '', q, offset);
  const options =
    query.isError || query.isFetching
      ? []
      : (query.data?.data.filter(
          (member) => member.user.status === 'active' && member.user.id !== pending.userId,
        ) ?? []);
  return (
    <div className="space-y-2">
      <label htmlFor="organization-successor" className="block text-sm">
        {text('继任负责人', 'Successor')}
      </label>
      <input
        aria-label={text('搜索本组织成员', 'Search organization members')}
        value={q}
        disabled={disabled}
        onChange={(event) => {
          setQ(event.target.value);
          setOffset(0);
        }}
        className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
      />
      <select
        id="organization-successor"
        value={pending.successorId}
        disabled={disabled || query.isFetching}
        onChange={(event) => change(event.target.value)}
        className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
      >
        <option value="">{text('请选择实际有效成员', 'Choose an actual active member')}</option>
        {pending.successorId && !options.some((m) => m.user.id === pending.successorId) && (
          <option value={pending.successorId}>{pending.successorId}</option>
        )}
        {options.map((m) => (
          <option key={m.user.id} value={m.user.id}>
            {m.user.display_name || m.user.username}
          </option>
        ))}
      </select>
      {query.error && (
        <ErrorBanner
          message={apiErrorMessage(
            query.error,
            text('继任成员加载失败', 'Could not load successors'),
          )}
        />
      )}
      <div className="flex justify-end gap-2">
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={!offset || disabled || query.isFetching}
          onClick={() => setOffset(Math.max(0, offset - 20))}
        >
          {text('上一页', 'Previous')}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={
            disabled ||
            !query.data ||
            offset + 20 >= query.data.total ||
            query.isFetching ||
            query.isError
          }
          onClick={() => setOffset(offset + 20)}
        >
          {text('下一页', 'Next')}
        </Button>
      </div>
    </div>
  );
}
export default function OrganizationOperationDialog({
  operations,
}: {
  operations: ReturnType<typeof useOrganizationOperations>;
}) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const { pending, setPending, busy, error, close, commit } = operations;
  const target = useOrganizationDetails(pending?.kind === 'transfer' ? pending.targetId : '');
  useEffect(() => {
    const organization = !target.isError && !target.isFetching ? target.data?.organization : null;
    if (!organization) return;
    setPending((current) =>
      current?.kind === 'transfer' && current.targetId === organization.id && !current.target
        ? {
            ...current,
            target: {
              id: organization.id,
              code: organization.code,
              name: organization.name,
              version: organization.version,
              parent_id: organization.parent_id,
            },
          }
        : current,
    );
  }, [target.data, target.isError, target.isFetching, setPending]);
  const update = (
    patch: Partial<Pick<PendingOrganizationOperation, 'reason' | 'handoverChoice' | 'successorId'>>,
  ) => setPending((current) => (current ? { ...current, ...patch } : current));
  return (
    <Dialog
      open={!!pending}
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent>
        <DialogTitle>
          {pending?.kind === 'transfer'
            ? text('加入或调动成员', 'Assign or transfer member')
            : pending?.kind === 'remove'
              ? text('移除组织成员', 'Remove organization member')
              : text('任免组织负责人', 'Change organization manager')}
        </DialogTitle>
        <DialogDescription>
          {pending?.userName} ·{' '}
          {text('变更不自动修改任何角色权限', 'Changes do not alter role permissions')}
        </DialogDescription>
        <form
          className="mt-4 space-y-4"
          onSubmit={(event) => {
            event.preventDefault();
            void commit();
          }}
        >
          {pending && pending.kind === 'transfer' && (
            <p className="text-sm">
              {pending.source?.name ?? text('未归属组织', 'Unassigned')} →{' '}
              {pending.target?.name ?? text('请选择目标', 'Choose a target')}
            </p>
          )}
          {pending && pending.kind === 'remove' && (
            <p className="text-sm">
              {pending.source?.name} · v{pending.membershipVersion}
            </p>
          )}
          {pending && pending.kind === 'transfer' && (
            <OrganizationReferencePicker
              kind="organization"
              label={text('调动目标组织', 'Target organization')}
              value={pending.targetId}
              disabled={busy}
              excludeId={pending.source?.id}
              onChange={(targetId) =>
                setPending((current) =>
                  current?.kind === 'transfer' ? { ...current, targetId, target: null } : current,
                )
              }
            />
          )}
          {pending?.kind === 'transfer' && target.error && (
            <ErrorBanner
              message={apiErrorMessage(
                target.error,
                text('目标组织加载失败', 'Could not load target organization'),
              )}
            />
          )}
          {pending && pending.isManager && pending.kind !== 'manager' && (
            <>
              <div>
                <label htmlFor="organization-handover" className="mb-1.5 block text-sm">
                  {text('当前负责人离任方式', 'Manager departure')}
                </label>
                <select
                  id="organization-handover"
                  required
                  disabled={busy}
                  value={pending.handoverChoice}
                  onChange={(event) => {
                    const value = event.target.value;
                    if (value === '' || value === 'successor' || value === 'resign')
                      update({ handoverChoice: value, successorId: '' });
                  }}
                  className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
                >
                  <option value="">
                    {text('请选择继任或明确卸任', 'Choose succession or explicit resignation')}
                  </option>
                  <option value="successor">
                    {text(
                      '指定本组织有效继任负责人',
                      'Assign an active successor in this organization',
                    )}
                  </option>
                  <option value="resign">
                    {text('明确卸任，不保留负责人', 'Explicitly resign without a manager')}
                  </option>
                </select>
              </div>
              {pending.handoverChoice === 'successor' && (
                <Successor
                  pending={pending}
                  disabled={busy}
                  change={(successorId) => update({ successorId })}
                />
              )}
            </>
          )}
          <div>
            <label htmlFor="organization-operation-reason" className="mb-1.5 block text-sm">
              {text('操作原因', 'Operation reason')}
            </label>
            <textarea
              id="organization-operation-reason"
              required
              maxLength={8192}
              disabled={busy}
              value={pending?.reason ?? ''}
              onChange={(event) => update({ reason: event.target.value })}
              className="w-full rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-3 text-sm"
            />
          </div>
          {error && <ErrorBanner message={error} />}
          <div className="flex justify-end gap-2">
            <Button type="button" variant="outline" onClick={close}>
              {text('关闭', 'Close')}
            </Button>
            <Button type="submit" disabled={busy}>
              {busy ? text('正在提交…', 'Submitting…') : text('确认', 'Confirm')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
