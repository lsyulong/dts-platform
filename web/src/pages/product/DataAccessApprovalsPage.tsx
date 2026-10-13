import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';
import { useAuthStore } from '../../stores/auth';
import ResourceOperationDialog from '../../product/ResourceOperationDialog';
import {
  actionAllowed,
  initialActionPayload,
  type PendingResourceAction,
} from '../../product/action-models';
import { statusText } from '../../product/ui-text';
import type { ProductResource } from '../../product/resource-types';
import { useDataAccessPage, type ApprovalPage } from './data-access-api';

export default function DataAccessApprovalsPage() {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <Approvals key={identity} />;
}

function Approvals() {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((state) => state.user);
  const canApprove =
    !!user?.permissions.includes('data_security:read') &&
    user.permissions.includes('data_security:approve');
  const [status, setStatus] = useState('pending');
  const [offset, setOffset] = useState(0);
  const [action, setAction] = useState<PendingResourceAction | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [receipt, setReceipt] = useState('');
  const keys = useRef(new Map<string, string>());
  const cache = useQueryClient();
  const page = useDataAccessPage<ApprovalPage>('approvals', status, offset, canApprove);
  const begin = (source: ProductResource, name: string) => {
    const record = { ...source, kind: 'data_access_request' };
    const key = `${record.id}:${record.version}:${name}`;
    const payload = initialActionPayload(record, name);
    payload.idempotency_key = keys.current.get(key) ?? payload.idempotency_key;
    keys.current.set(key, String(payload.idempotency_key));
    setAction({ record, name, payload });
    setError('');
    setReceipt('');
  };
  const commit = async () => {
    if (!action || busy || !canApprove || action.record.created_by === user?.id) return;
    const session = captureSession();
    setBusy(true);
    setError('');
    try {
      const response = await client.post<{ result: { decision_id: string } }>(
        `/product/data-access/requests/${encodeURIComponent(action.record.id)}/actions/${action.name}`,
        { expected_version: action.record.version, payload: action.payload },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (isCurrentSession(session)) {
        keys.current.delete(`${action.record.id}:${action.record.version}:${action.name}`);
        setReceipt(response.data.result.decision_id);
        setAction(null);
        void cache.invalidateQueries({ queryKey: ['data-access'] });
        void cache.invalidateQueries({ queryKey: ['product-resources'] });
      }
    } catch (caught) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(caught, text('审批提交失败', 'Could not submit the decision')));
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">{text('权限审批', 'Data access approvals')}</h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {text(
            '审批真实表和字段访问申请；授权同时受失效时间、撤销状态和源结构版本约束',
            'Approve access to actual tables and columns; expiry, revocation and source schema changes affect grants',
          )}
        </p>
      </div>
      {!canApprove && (
        <ErrorBanner
          message={text('当前账号没有权限审批权限', 'Your account cannot approve requests')}
        />
      )}
      {!!receipt && (
        <p role="status" className="rounded-md border border-[var(--color-accent)] p-3 text-sm">
          {text('审批决定已保存', 'Decision saved')}: {receipt}
        </p>
      )}
      {error && !action && <ErrorBanner message={error} />}
      <section className="data-panel">
        <div className="data-toolbar">
          <select
            aria-label={text('申请状态', 'Request status')}
            value={status}
            onChange={(event) => {
              setStatus(event.target.value);
              setOffset(0);
            }}
            className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
          >
            <option value="">{text('全部状态', 'All statuses')}</option>
            {['pending', 'approved', 'rejected', 'withdrawn', 'revoked'].map((value) => (
              <option key={value} value={value}>
                {statusText(value, i18n.language)}
              </option>
            ))}
          </select>
          <Button
            variant="outline"
            disabled={!canApprove || page.isFetching}
            onClick={() => void page.refetch()}
          >
            {text('刷新', 'Refresh')}
          </Button>
        </div>
        {page.error && (
          <div className="p-4">
            <ErrorBanner
              message={apiErrorMessage(page.error, text('申请加载失败', 'Could not load requests'))}
            />
          </div>
        )}
        <div className="overflow-auto">
          <table className="w-full text-left text-sm">
            <thead className="bg-[var(--color-bg-secondary)]">
              <tr>
                {[
                  text('申请 / 申请人', 'Request / requester'),
                  text('表对象 / 字段', 'Object / columns'),
                  text('用途 / 失效时间', 'Purpose / expiry'),
                  text('状态', 'Status'),
                  text('审批操作', 'Decisions'),
                ].map((label) => (
                  <th key={label} className="p-4 font-medium">
                    {label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {canApprove &&
                page.data?.data.map((record) => (
                  <tr key={record.id} className="border-t border-[var(--color-border-default)]">
                    <td className="p-4">
                      <strong>{record.name}</strong>
                      <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                        {record.created_by}
                      </p>
                    </td>
                    <td className="max-w-80 p-4">
                      <p className="break-all text-xs">
                        {String(record.properties.object_id ?? '')} · v
                        {String(record.properties.metadata_version ?? '')}
                      </p>
                      <p className="mt-1 break-words">
                        {Array.isArray(record.properties.columns)
                          ? record.properties.columns.join(', ')
                          : '—'}
                      </p>
                    </td>
                    <td className="max-w-80 p-4">
                      <p className="whitespace-pre-wrap break-words">
                        {String(record.properties.purpose ?? '')}
                      </p>
                      <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                        {String(record.properties.expires_at ?? '')}
                      </p>
                    </td>
                    <td className="p-4">{statusText(record.status, i18n.language)}</td>
                    <td className="p-4">
                      <div className="flex gap-2">
                        {['approve', 'reject', 'revoke']
                          .filter((name) =>
                            actionAllowed({ ...record, kind: 'data_access_request' }, name),
                          )
                          .map((name) => (
                            <Button
                              key={name}
                              size="sm"
                              variant="outline"
                              disabled={busy || record.created_by === user?.id}
                              onClick={() => begin(record, name)}
                            >
                              {statusText(name, i18n.language)}
                            </Button>
                          ))}
                      </div>
                      {record.created_by === user?.id && (
                        <p className="mt-2 text-xs text-[var(--color-text-tertiary)]">
                          {text('不能审批自己的申请', 'You cannot approve your own request')}
                        </p>
                      )}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
        {canApprove && !page.data?.data.length && (
          <p className="py-14 text-center text-sm text-[var(--color-text-tertiary)]">
            {page.isPending
              ? text('正在加载', 'Loading')
              : text('没有符合条件的权限申请', 'No requests match this filter')}
          </p>
        )}
        <div className="flex justify-between border-t border-[var(--color-border-default)] p-4 text-sm">
          <span>
            {canApprove && page.data ? `${page.data.total} ${text('条申请', 'requests')}` : '—'}
          </span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={!canApprove || !offset || page.isFetching}
              onClick={() => setOffset(Math.max(0, offset - 20))}
            >
              {text('上一页', 'Previous')}
            </Button>
            <Button
              variant="outline"
              disabled={
                !canApprove || !page.data || offset + 20 >= page.data.total || page.isFetching
              }
              onClick={() => setOffset(offset + 20)}
            >
              {text('下一页', 'Next')}
            </Button>
          </div>
        </div>
      </section>
      <ResourceOperationDialog
        deletion={null}
        action={action}
        busy={busy}
        error={error}
        onChange={(payload) => setAction((current) => (current ? { ...current, payload } : null))}
        onClose={() => {
          setAction(null);
          setError('');
        }}
        onCommit={() => void commit()}
      />
    </div>
  );
}
