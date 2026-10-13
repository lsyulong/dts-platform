import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { useAuthStore } from '../../stores/auth';
import { captureSession, isCurrentSession } from '../../lib/session';
import { apiErrorMessage } from '../../lib/apiError';
import { selectionSessionScope } from '../../product/business-editors/option-sources';
import { statusText } from '../../product/ui-text';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../components/ui/dialog';
import type { ProductJob } from './jobs-api';
import JobNodeResult from './JobNodeResult';

export interface BiShare {
  id: string;
  tenant_id: string;
  report_id: string;
  report_version: number;
  job_id: string;
  node_id: string;
  owner_id: string;
  recipient_id: string;
  status: 'active' | 'revoked';
  version: number;
  expires_at: string;
  is_expired: boolean;
  created_at: string;
  revoked_at: string | null;
}
interface Shared {
  share: BiShare;
  job: ProductJob;
}

export default function BiSharesPage() {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <Shares key={identity} />;
}
function Shares() {
  const { i18n } = useTranslation(),
    text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((state) => state.user),
    token = useAuthStore((state) => state.token),
    canRead = !!user?.permissions.includes('bi:read'),
    canWrite = !!user?.permissions.includes('bi:write');
  const [params, setParams] = useSearchParams(),
    [offset, setOffset] = useState(0),
    [pending, setPending] = useState<BiShare | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState('');
  const selected = params.get('share') ?? '',
    cache = useQueryClient();
  const permissionScope = JSON.stringify(user?.permissions ?? []);
  const list = useQuery({
    queryKey: [
      'bi-shares',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      permissionScope,
      offset,
    ],
    queryFn: ({ signal }) =>
      client
        .get<{ data: BiShare[]; total: number }>('/product/bi-shares', {
          params: { limit: 20, offset },
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((response) => response.data),
    enabled: canRead && !!token,
    refetchInterval: 30000,
  });
  const open = useQuery({
    queryKey: [
      'bi-share-open',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      permissionScope,
      selected,
    ],
    queryFn: ({ signal }) =>
      client
        .get<Shared>(`/product/bi-shares/${encodeURIComponent(selected)}/open`, {
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((response) => response.data),
    enabled: canRead && !!token && !!selected,
    refetchInterval: 15000,
  });
  const listed = list.data?.data.find((share) => share.id === selected);
  const candidate = open.data;
  const candidateNode = candidate?.job.nodes.find((node) => node.id === candidate.share.node_id);
  const isActive = (share: BiShare) =>
    share.status === 'active' && !share.is_expired && Date.parse(share.expires_at) > Date.now();
  const opened =
    !list.error &&
    !open.error &&
    candidate?.share.id === selected &&
    isActive(candidate.share) &&
    (!listed || (isActive(listed) && listed.version === candidate.share.version)) &&
    candidate.job.id === candidate.share.job_id &&
    candidate.job.source_kind === 'bi_report' &&
    candidate.job.source_id === candidate.share.report_id &&
    candidate.job.source_version === candidate.share.report_version &&
    candidate.job.status === 'succeeded' &&
    candidate.job.result_access === 'authorized' &&
    candidateNode?.status === 'succeeded' &&
    candidateNode.result_access === 'authorized'
      ? candidate
      : null;
  const revoke = async () => {
    if (!pending || busy || !canWrite || pending.owner_id !== user?.id) return;
    const session = captureSession();
    setBusy(true);
    setError('');
    try {
      await client.post(
        `/product/bi-shares/${encodeURIComponent(pending.id)}/revoke`,
        { expected_version: pending.version },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (isCurrentSession(session)) {
        setPending(null);
        setParams({});
        void cache.invalidateQueries({ queryKey: ['bi-shares'] });
        void cache.invalidateQueries({ queryKey: ['bi-share-open'] });
      }
    } catch (caught) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(caught, text('撤销分享失败', 'Could not revoke share')));
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  const download = async (format: 'json' | 'csv') => {
    if (!opened || busy || !canRead) return;
    const session = captureSession();
    setBusy(true);
    setError('');
    try {
      const response = await client.post<Blob>(
        `/product/bi-shares/${encodeURIComponent(opened.share.id)}/export`,
        { format },
        { responseType: 'blob', headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (isCurrentSession(session)) {
        const url = URL.createObjectURL(response.data),
          link = document.createElement('a');
        link.href = url;
        link.download = `bi-share-${opened.share.id}.${format}`;
        link.click();
        URL.revokeObjectURL(url);
      }
    } catch (caught) {
      if (isCurrentSession(session))
        setError(
          apiErrorMessage(caught, text('分享结果导出失败', 'Could not export shared result')),
        );
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  if (!canRead)
    return (
      <ErrorBanner
        message={text('当前账号没有 BI 查看权限', 'Your account cannot read BI reports')}
      />
    );
  const node = opened?.job.nodes.find((item) => item.id === opened.share.node_id);
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">{text('报表分享', 'Report shares')}</h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {text(
            '仅展示由你创建或指定你接收的分享，结果按当前权限和脱敏规则读取。',
            'Shares created by you or assigned to you. Results use current access and masking policies.',
          )}
        </p>
      </div>
      {(error || list.error || open.error) && (
        <ErrorBanner
          message={
            error ||
            apiErrorMessage(list.error ?? open.error, text('分享读取失败', 'Could not read share'))
          }
        />
      )}
      <section className="data-panel">
        <div className="data-toolbar">
          <Button variant="outline" disabled={list.isFetching} onClick={() => void list.refetch()}>
            {text('刷新', 'Refresh')}
          </Button>
        </div>
        <div className="overflow-auto">
          <table className="data-table">
            <thead>
              <tr>
                {[
                  text('报表 / 版本', 'Report / version'),
                  text('接收用户', 'Recipient'),
                  text('到期时间', 'Expires at'),
                  text('状态', 'Status'),
                  text('操作', 'Actions'),
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {list.data?.data.map((share) => (
                <tr key={share.id}>
                  <td className="break-all text-xs">
                    {share.report_id} · v{share.report_version}
                  </td>
                  <td className="break-all text-xs">{share.recipient_id}</td>
                  <td>{new Date(share.expires_at).toLocaleString(i18n.language)}</td>
                  <td>
                    {share.is_expired
                      ? text('已到期', 'Expired')
                      : statusText(share.status, i18n.language)}
                  </td>
                  <td>
                    <div className="flex gap-2">
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={share.status !== 'active' || share.is_expired}
                        onClick={() => {
                          setError('');
                          setParams({ share: share.id });
                        }}
                      >
                        {text('查看结果', 'Open result')}
                      </Button>
                      {canWrite && share.owner_id === user?.id && share.status === 'active' && (
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={busy}
                          onClick={() => {
                            setPending(share);
                            setError('');
                          }}
                        >
                          {text('撤销分享', 'Revoke share')}
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {!list.data?.data.length && (
          <p className="py-12 text-center text-sm text-[var(--color-text-tertiary)]">
            {list.isPending
              ? text('正在加载…', 'Loading…')
              : text(
                  '暂无分享记录；可在实际成功的 BI 运行中创建分享',
                  'No shares. Create one from a successful BI run.',
                )}
          </p>
        )}
        <div className="flex justify-between p-4 text-sm">
          <span>{list.data?.total ?? '—'}</span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={!offset || list.isFetching}
              onClick={() => setOffset(Math.max(0, offset - 20))}
            >
              {text('上一页', 'Previous')}
            </Button>
            <Button
              variant="outline"
              disabled={!list.data || offset + 20 >= list.data.total || list.isFetching}
              onClick={() => setOffset(offset + 20)}
            >
              {text('下一页', 'Next')}
            </Button>
          </div>
        </div>
      </section>
      {selected && open.isPending && (
        <p role="status" className="text-sm">
          {text('正在核对分享与数据权限…', 'Checking share and data access…')}
        </p>
      )}
      {selected && open.data && !open.error && !opened && (
        <ErrorBanner
          message={text(
            '分享已失效，或当前报表、任务及数据权限不允许读取该结果。',
            'This share is unavailable under the current report, run and data permissions.',
          )}
        />
      )}
      {opened && !open.error && node?.result && (
        <section className="data-panel p-5">
          <div className="flex flex-wrap justify-between gap-3">
            <h2 className="font-medium">
              {text('分享的实际运行结果', 'Shared run result')} · v{opened.share.report_version}
            </h2>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={() => void download('json')}
              >
                {text('导出 JSON', 'Export JSON')}
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={() => void download('csv')}
              >
                {text('导出 CSV', 'Export CSV')}
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  setParams({});
                }}
              >
                {text('收起', 'Close')}
              </Button>
            </div>
          </div>
          <JobNodeResult result={node.result} />
        </section>
      )}
      <Dialog
        open={!!pending}
        onOpenChange={(value) => {
          if (!value && !busy) {
            setPending(null);
            setError('');
          }
        }}
      >
        <DialogContent>
          <DialogTitle>{text('撤销分享', 'Revoke share')}</DialogTitle>
          <DialogDescription>
            {text(
              '撤销后接收人不能继续通过此分享读取结果。',
              'The recipient will no longer read results through this share.',
            )}
          </DialogDescription>
          {error && <ErrorBanner message={error} />}
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="outline" disabled={busy} onClick={() => setPending(null)}>
              {text('取消', 'Cancel')}
            </Button>
            <Button disabled={busy} onClick={() => void revoke()}>
              {text('确认撤销', 'Confirm revocation')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
