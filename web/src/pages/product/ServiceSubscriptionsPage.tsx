import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { useAuthStore } from '../../stores/auth';
import type { ProductModule } from '../../product/catalog-types';
import type { ResourcePage } from '../../product/resource-types';
import ProductModulePage from '../../product/ProductModulePage';
import ResourceOperationDialog from '../../product/ResourceOperationDialog';
import {
  actionAllowed,
  initialActionPayload,
  type PendingResourceAction,
} from '../../product/action-models';
import { statusText } from '../../product/ui-text';
import { captureSession, isCurrentSession } from '../../lib/session';
import { apiErrorMessage } from '../../lib/apiError';
import { useServicePage } from './service-access-api';

export default function ServiceSubscriptionsPage({ module }: { module: ProductModule }) {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <Subscriptions key={identity} module={module} />;
}
function Subscriptions({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation(),
    [tab, setTab] = useState<'mine' | 'approvals'>('mine');
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((state) => state.user),
    canApprove =
      !!user?.permissions.includes('services:read') &&
      !!user?.permissions.includes('services:approve');
  const [status, setStatus] = useState('pending'),
    [offset, setOffset] = useState(0);
  const [action, setAction] = useState<PendingResourceAction | null>(null),
    [error, setError] = useState(''),
    [receipt, setReceipt] = useState(''),
    [busy, setBusy] = useState(false);
  const keys = useRef(new Map<string, string>()),
    cache = useQueryClient();
  const approvals = useServicePage<ResourcePage>(
    '/product/service-access/approvals',
    { status: status || undefined, limit: 20, offset },
    canApprove && tab === 'approvals',
  );
  const commit = async () => {
    if (!action || busy || !canApprove) return;
    const session = captureSession();
    setBusy(true);
    setError('');
    try {
      const response = await client.post<{ result: { decision_id: string } }>(
        `/product/resources/service_subscription/${encodeURIComponent(action.record.id)}/actions/${action.name}`,
        { expected_version: action.record.version, payload: action.payload },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (isCurrentSession(session)) {
        setReceipt(response.data.result.decision_id);
        keys.current.delete(`${action.record.id}:${action.record.version}:${action.name}`);
        setAction(null);
        void cache.invalidateQueries({ queryKey: ['service-access'] });
        void cache.invalidateQueries({ queryKey: ['product-resources'] });
      }
    } catch (caught) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(caught, text('审批提交失败', 'Could not submit decision')));
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  return (
    <div className="space-y-5">
      <div
        role="tablist"
        aria-label={text('服务订阅', 'Service subscriptions')}
        className="flex gap-4 border-b border-[var(--color-border-default)]"
      >
        {(['mine', 'approvals'] as const).map((value) => (
          <button
            key={value}
            role="tab"
            aria-selected={tab === value}
            disabled={value === 'approvals' && !canApprove}
            onClick={() => setTab(value)}
            className={`border-b-2 px-3 py-3 text-sm disabled:opacity-40 ${tab === value ? 'border-[var(--color-accent)] text-[var(--color-accent)]' : 'border-transparent text-[var(--color-text-secondary)]'}`}
          >
            {value === 'mine'
              ? text('我的服务订阅', 'My subscriptions')
              : text('订阅审批', 'Subscription approvals')}
          </button>
        ))}
      </div>
      {tab === 'mine' ? (
        <ProductModulePage module={module} />
      ) : (
        <>
          <div>
            <h1 className="text-xl font-semibold">
              {text('服务订阅审批', 'Service subscription approvals')}
            </h1>
            <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
              {text(
                '审批应用对具体 API 发布版本的访问；应用负责人仍须具有当前数据访问授权。',
                'Approve application access to a specific API publication. Its owner must retain current data access.',
              )}
            </p>
          </div>
          {receipt && (
            <p role="status" className="rounded-md border border-[var(--color-accent)] p-3 text-sm">
              {text('审批决定已保存', 'Decision saved')}: {receipt}
            </p>
          )}
          {error && !action && <ErrorBanner message={error} />}
          <section className="data-panel">
            <div className="data-toolbar">
              <select
                aria-label={text('订阅状态', 'Subscription status')}
                value={status}
                onChange={(event) => {
                  setStatus(event.target.value);
                  setOffset(0);
                }}
                className="rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-2 text-sm"
              >
                <option value="">{text('全部状态', 'All statuses')}</option>
                {['pending', 'approved', 'rejected', 'revoked'].map((value) => (
                  <option key={value} value={value}>
                    {statusText(value, i18n.language)}
                  </option>
                ))}
              </select>
              <Button
                variant="outline"
                disabled={approvals.isFetching}
                onClick={() => void approvals.refetch()}
              >
                {text('刷新', 'Refresh')}
              </Button>
            </div>
            {approvals.error && (
              <div className="p-4">
                <ErrorBanner
                  message={apiErrorMessage(
                    approvals.error,
                    text('订阅加载失败', 'Could not load subscriptions'),
                  )}
                />
              </div>
            )}
            <div className="overflow-auto">
              <table className="data-table">
                <thead>
                  <tr>
                    {[
                      text('申请 / 负责人', 'Request / owner'),
                      text('API / 版本', 'API / version'),
                      text('用途 / 到期', 'Purpose / expiry'),
                      text('状态', 'Status'),
                      text('审批操作', 'Decisions'),
                    ].map((label) => (
                      <th key={label}>{label}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {approvals.data?.data.map((record) => {
                    const self = [
                      record.created_by,
                      record.properties.owner_id,
                      record.properties.requester_id,
                    ].includes(user?.id ?? '');
                    return (
                      <tr key={record.id}>
                        <td>
                          <strong>{record.name}</strong>
                          <p className="mt-1 break-all text-xs">
                            {String(record.properties.owner_id ?? record.created_by)}
                          </p>
                        </td>
                        <td className="break-all text-xs">
                          {String(record.properties.api_id)} · v
                          {String(record.properties.api_version)}
                        </td>
                        <td>
                          <p className="max-w-80 whitespace-pre-wrap break-words">
                            {String(record.properties.purpose)}
                          </p>
                          <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                            {new Date(String(record.properties.expires_at)).toLocaleString(
                              i18n.language,
                            )}
                          </p>
                        </td>
                        <td>{statusText(record.status, i18n.language)}</td>
                        <td>
                          <div className="flex gap-2">
                            {['approve', 'reject', 'revoke']
                              .filter((name) => actionAllowed(record, name))
                              .map((name) => (
                                <Button
                                  key={name}
                                  size="sm"
                                  variant="outline"
                                  disabled={busy || self}
                                  onClick={() => {
                                    const id = `${record.id}:${record.version}:${name}`;
                                    const payload = initialActionPayload(record, name);
                                    payload.idempotency_key =
                                      keys.current.get(id) ?? payload.idempotency_key;
                                    keys.current.set(id, String(payload.idempotency_key));
                                    setAction({ record, name, payload });
                                    setError('');
                                    setReceipt('');
                                  }}
                                >
                                  {statusText(name, i18n.language)}
                                </Button>
                              ))}
                          </div>
                          {self && (
                            <p className="mt-2 text-xs text-[var(--color-text-tertiary)]">
                              {text(
                                '不能审批自己申请或负责的应用',
                                'You cannot approve your own request or application',
                              )}
                            </p>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            {!approvals.data?.data.length && (
              <p className="py-12 text-center text-sm text-[var(--color-text-tertiary)]">
                {approvals.isPending
                  ? text('正在加载…', 'Loading…')
                  : text('没有符合条件的订阅', 'No matching subscriptions')}
              </p>
            )}
            <div className="flex justify-between p-4 text-sm">
              <span>{approvals.data?.total ?? '—'}</span>
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  disabled={!offset || approvals.isFetching}
                  onClick={() => setOffset(Math.max(0, offset - 20))}
                >
                  {text('上一页', 'Previous')}
                </Button>
                <Button
                  variant="outline"
                  disabled={
                    !approvals.data || offset + 20 >= approvals.data.total || approvals.isFetching
                  }
                  onClick={() => setOffset(offset + 20)}
                >
                  {text('下一页', 'Next')}
                </Button>
              </div>
            </div>
          </section>
        </>
      )}
      <ResourceOperationDialog
        deletion={null}
        action={action}
        busy={busy}
        error={error}
        onChange={(payload) => setAction((current) => (current ? { ...current, payload } : null))}
        onClose={() => {
          if (!busy) {
            setAction(null);
            setError('');
          }
        }}
        onCommit={() => void commit()}
      />
    </div>
  );
}
