import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import ReferencePicker from '../../product/ReferencePicker';
import { useAuthStore } from '../../stores/auth';
import { apiErrorMessage } from '../../lib/apiError';
import { statusText } from '../../product/ui-text';
import { useServicePage } from './service-access-api';

interface ServiceCall {
  id: string;
  tenant_id: string;
  owner_id: string;
  application_id: string;
  credential_id: string;
  api_id: string | null;
  target_hash: string;
  outcome: string;
  reason_code: string | null;
  started_at: string;
  finished_at: string | null;
  duration_ms: number | null;
  row_count: number | null;
}
const reasons: Record<string, [string, string]> = {
  rate_limited: ['超过每分钟限额', 'Per-minute quota exceeded'],
  credential_unavailable: ['凭据已撤销或不可用', 'Credential revoked or unavailable'],
  credential_expired: ['凭据已到期', 'Credential expired'],
  application_inactive: ['应用已归档或停用', 'Application archived or inactive'],
  application_expired: ['应用已到期', 'Application expired'],
  origin_denied: ['请求来源未获准', 'Origin not allowed'],
  subscription_unavailable: ['没有有效订阅授权', 'No active subscription'],
  subscription_expired: ['订阅已到期', 'Subscription expired'],
  api_unavailable: ['API 不可用', 'API unavailable'],
  api_version_changed: ['API 发布版本已变化', 'API publication changed'],
  owner_inactive: ['应用负责人已停用', 'Application owner inactive'],
};

export default function ServiceCallsPage() {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <Calls key={identity} />;
}
function Calls() {
  const { i18n } = useTranslation();
  const zh = i18n.language.startsWith('zh'),
    text = (cn: string, en: string) => (zh ? cn : en);
  const canRead = useAuthStore((state) => state.user?.permissions.includes('services:read'));
  const [applicationId, setApplicationId] = useState(''),
    [apiId, setApiId] = useState(''),
    [offset, setOffset] = useState(0),
    [selected, setSelected] = useState('');
  const calls = useServicePage<{ data: ServiceCall[]; total: number }>(
    '/product/service-access/calls',
    { application_id: applicationId || undefined, api_id: apiId || undefined, limit: 20, offset },
    !!canRead,
  );
  const detail = calls.data?.data.find((call) => call.id === selected);
  if (!canRead)
    return (
      <ErrorBanner
        message={text('当前账号没有服务查看权限', 'Your account cannot read services')}
      />
    );
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">{text('我的调用', 'My API calls')}</h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {text(
            '查看应用发起的实际 API 调用、授权拒绝和限额结果',
            'Inspect actual application API calls, authorization denials and quota outcomes',
          )}
        </p>
      </div>
      <section className="data-panel p-5">
        <div className="grid items-end gap-4 sm:grid-cols-3">
          <ReferencePicker
            kind="service_application"
            label={text('调用应用（可选）', 'Application (optional)')}
            value={applicationId}
            onChange={(value) => {
              setApplicationId(value);
              setOffset(0);
              setSelected('');
            }}
          />
          <ReferencePicker
            kind="data_api"
            label={text('数据 API（可选）', 'Data API (optional)')}
            value={apiId}
            onChange={(value) => {
              setApiId(value);
              setOffset(0);
              setSelected('');
            }}
          />
          <Button
            variant="outline"
            disabled={calls.isFetching}
            onClick={() => void calls.refetch()}
          >
            {text('刷新', 'Refresh')}
          </Button>
        </div>
      </section>
      {calls.error && (
        <ErrorBanner
          message={apiErrorMessage(calls.error, text('调用记录加载失败', 'Could not load calls'))}
        />
      )}
      <section className="data-panel">
        <div className="overflow-auto">
          <table className="data-table">
            <thead>
              <tr>
                {[
                  text('调用时间 / ID', 'Time / call ID'),
                  text('应用 / API', 'Application / API'),
                  text('结果', 'Outcome'),
                  text('耗时', 'Duration'),
                  text('返回行数', 'Returned rows'),
                  text('操作', 'Actions'),
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {calls.data?.data.map((call) => (
                <tr key={call.id}>
                  <td>
                    {new Date(call.started_at).toLocaleString(i18n.language)}
                    <p className="mt-1 break-all text-xs text-[var(--color-text-tertiary)]">
                      {call.id}
                    </p>
                  </td>
                  <td className="break-all text-xs">
                    <p>{call.application_id}</p>
                    <p className="mt-1 text-[var(--color-text-tertiary)]">
                      {call.api_id ?? text('目标未获准', 'Target not authorized')}
                    </p>
                  </td>
                  <td>
                    {statusText(call.outcome, i18n.language)}
                    {call.reason_code && (
                      <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
                        {reasons[call.reason_code]?.[zh ? 0 : 1] ?? call.reason_code}
                      </p>
                    )}
                  </td>
                  <td>{call.duration_ms === null ? '—' : `${call.duration_ms} ms`}</td>
                  <td>
                    {call.row_count === null ? '—' : call.row_count.toLocaleString(i18n.language)}
                  </td>
                  <td>
                    <Button variant="outline" size="sm" onClick={() => setSelected(call.id)}>
                      {text('详情', 'Details')}
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {!calls.data?.data.length && (
          <p className="py-12 text-center text-sm text-[var(--color-text-tertiary)]">
            {calls.isPending
              ? text('正在加载…', 'Loading…')
              : text('暂无符合条件的实际调用记录', 'No matching call records')}
          </p>
        )}
        <div className="flex justify-between p-4 text-sm">
          <span>{calls.data?.total ?? '—'}</span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={!offset || calls.isFetching}
              onClick={() => {
                setOffset(Math.max(0, offset - 20));
                setSelected('');
              }}
            >
              {text('上一页', 'Previous')}
            </Button>
            <Button
              variant="outline"
              disabled={!calls.data || offset + 20 >= calls.data.total || calls.isFetching}
              onClick={() => {
                setOffset(offset + 20);
                setSelected('');
              }}
            >
              {text('下一页', 'Next')}
            </Button>
          </div>
        </div>
      </section>
      {detail && (
        <section className="data-panel space-y-3 p-5">
          <div className="flex justify-between">
            <h2 className="font-medium">{text('调用记录详情', 'Call details')}</h2>
            <Button variant="outline" size="sm" onClick={() => setSelected('')}>
              {text('收起', 'Close')}
            </Button>
          </div>
          <dl className="grid gap-3 text-sm sm:grid-cols-2">
            {[
              [text('调用 ID', 'Call ID'), detail.id],
              [text('应用负责人', 'Application owner'), detail.owner_id],
              [text('凭据 ID', 'Credential ID'), detail.credential_id],
              [
                text('完成时间', 'Finished at'),
                detail.finished_at
                  ? new Date(detail.finished_at).toLocaleString(i18n.language)
                  : '—',
              ],
            ].map(([label, value]) => (
              <div key={label}>
                <dt className="text-xs text-[var(--color-text-tertiary)]">{label}</dt>
                <dd className="mt-1 break-all">{value}</dd>
              </div>
            ))}
          </dl>
          {detail.outcome === 'pending' && (
            <p className="text-xs text-[var(--color-text-tertiary)]">
              {text(
                '调用尚未记录终态；连接中断或进程退出时不能据此判定成功。',
                'No terminal outcome has been recorded. A disconnected request or process exit does not establish success.',
              )}
            </p>
          )}
        </section>
      )}
    </div>
  );
}
