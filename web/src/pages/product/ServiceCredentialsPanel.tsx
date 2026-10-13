import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../components/ui/dialog';
import { useAuthStore } from '../../stores/auth';
import ReferencePicker from '../../product/ReferencePicker';
import ExpiryInput from '../../product/ExpiryInput';
import type { ProductResource } from '../../product/resource-types';
import { captureSession, isCurrentSession } from '../../lib/session';
import { apiErrorMessage } from '../../lib/apiError';
import { useServicePage, type ServiceCredential } from './service-access-api';

interface Issued {
  application: ProductResource;
  credential: ServiceCredential;
  secret: string | null;
  secret_not_retrievable: boolean;
  replayed: boolean;
}
type Pending =
  | { kind: 'issue'; key: string; application: ProductResource; expires_at: unknown }
  | { kind: 'revoke'; key: string; credential: ServiceCredential };

export default function ServiceCredentialsPanel() {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <Credentials key={identity} />;
}

function Credentials() {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const canRead = useAuthStore((state) => !!state.user?.permissions.includes('services:read')),
    canWrite = useAuthStore((state) => !!state.user?.permissions.includes('services:write'));
  const [applicationId, setApplicationId] = useState(''),
    [offset, setOffset] = useState(0);
  const [pending, setPending] = useState<Pending | null>(null),
    [issued, setIssued] = useState<Issued | null>(null);
  const [error, setError] = useState(''),
    [busy, setBusy] = useState(false),
    [copied, setCopied] = useState(false);
  const keys = useRef(new Map<string, string>()),
    cache = useQueryClient();
  const application = useServicePage<ProductResource>(
    `/product/resources/service_application/${encodeURIComponent(applicationId)}`,
    {},
    canRead && !!applicationId,
  );
  const credentials = useServicePage<{ data: ServiceCredential[]; total: number }>(
    `/product/service-access/applications/${encodeURIComponent(applicationId)}/credentials`,
    { limit: 20, offset },
    canRead && !!applicationId,
  );
  const begin = (next: Pending) => {
    const identity =
      next.kind === 'issue'
        ? `issue:${next.application.id}:${next.application.version}`
        : `revoke:${next.credential.id}:${next.credential.version}`;
    next.key = keys.current.get(identity) ?? next.key;
    keys.current.set(identity, next.key);
    setPending(next);
    setError('');
    setIssued(null);
    setCopied(false);
  };
  const commit = async () => {
    if (!pending || busy || !canWrite) return;
    const session = captureSession();
    setBusy(true);
    setError('');
    try {
      if (pending.kind === 'issue') {
        const response = await client.post<Issued>(
          `/product/service-access/applications/${encodeURIComponent(pending.application.id)}/credentials`,
          {
            expected_version: pending.application.version,
            idempotency_key: pending.key,
            expires_at: typeof pending.expires_at === 'string' ? pending.expires_at : null,
          },
          { headers: { Authorization: `Bearer ${session.token}` } },
        );
        if (isCurrentSession(session)) {
          setIssued(response.data);
          keys.current.delete(`issue:${pending.application.id}:${pending.application.version}`);
        }
      } else {
        await client.post(
          `/product/service-access/applications/${encodeURIComponent(applicationId)}/credentials/${encodeURIComponent(pending.credential.id)}/revoke`,
          { expected_version: pending.credential.version, idempotency_key: pending.key },
          { headers: { Authorization: `Bearer ${session.token}` } },
        );
        if (isCurrentSession(session))
          keys.current.delete(`revoke:${pending.credential.id}:${pending.credential.version}`);
      }
      if (isCurrentSession(session)) {
        setPending(null);
        void cache.invalidateQueries({ queryKey: ['service-access'] });
        void cache.invalidateQueries({ queryKey: ['product-resources'] });
      }
    } catch (caught) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(caught, text('凭据操作失败', 'Credential operation failed')));
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  const copy = async () => {
    if (!issued?.secret) return;
    const session = captureSession();
    try {
      await navigator.clipboard.writeText(issued.secret);
      if (isCurrentSession(session)) setCopied(true);
    } catch {
      if (isCurrentSession(session))
        setError(text('复制失败，请手动保存凭据', 'Copy failed; save the credential manually'));
    }
  };
  if (!canRead)
    return (
      <ErrorBanner
        message={text('当前账号没有服务查看权限', 'Your account cannot read services')}
      />
    );
  return (
    <div className="space-y-5">
      <section className="data-panel p-5">
        <div className="grid items-end gap-4 sm:grid-cols-2">
          <ReferencePicker
            kind="service_application"
            label={text('调用应用', 'Application')}
            value={applicationId}
            onChange={(value) => {
              setApplicationId(value);
              setOffset(0);
              setPending(null);
              setIssued(null);
              setError('');
            }}
            disabled={busy}
          />
          <Button
            disabled={
              !canWrite || busy || !application.data || application.data.status !== 'active'
            }
            onClick={() => {
              if (application.data)
                begin({
                  kind: 'issue',
                  key: crypto.randomUUID(),
                  application: application.data,
                  expires_at: undefined,
                });
            }}
          >
            {text('签发新的调用凭据', 'Issue a new credential')}
          </Button>
        </div>
        <p className="mt-3 text-xs text-[var(--color-text-tertiary)]">
          {text(
            '凭据首次签发时显示一次。撤销后立即停止新的授权调用；应用仍需获得具体 API 的订阅审批。',
            'Credentials are shown once at issuance. Revocation stops new authorized calls; API subscription approval is also required.',
          )}
        </p>
      </section>
      {((error && !pending) || application.error || credentials.error) && (
        <ErrorBanner
          message={
            error ||
            apiErrorMessage(
              application.error ?? credentials.error,
              text('凭据加载失败', 'Could not load credentials'),
            )
          }
        />
      )}
      <section className="data-panel">
        <div className="data-toolbar">
          <h2 className="font-medium">{text('已签发凭据', 'Issued credentials')}</h2>
          <Button
            variant="outline"
            disabled={!applicationId || credentials.isFetching}
            onClick={() => void credentials.refetch()}
          >
            {text('刷新', 'Refresh')}
          </Button>
        </div>
        <div className="overflow-auto">
          <table className="data-table">
            <thead>
              <tr>
                {[
                  text('凭据 ID', 'Credential ID'),
                  text('签发时间', 'Issued at'),
                  text('到期时间', 'Expires at'),
                  text('状态', 'Status'),
                  text('操作', 'Actions'),
                ].map((label) => (
                  <th key={label}>{label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {credentials.data?.data.map((credential) => (
                <tr key={credential.id}>
                  <td className="break-all text-xs">{credential.id}</td>
                  <td>{new Date(credential.created_at).toLocaleString(i18n.language)}</td>
                  <td>{new Date(credential.expires_at).toLocaleString(i18n.language)}</td>
                  <td>
                    {credential.revoked_at
                      ? text('已撤销', 'Revoked')
                      : Date.parse(credential.expires_at) <= Date.now()
                        ? text('已到期', 'Expired')
                        : text('有效', 'Active')}
                  </td>
                  <td>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!canWrite || busy || !!credential.revoked_at}
                      onClick={() =>
                        begin({ kind: 'revoke', key: crypto.randomUUID(), credential })
                      }
                    >
                      {text('撤销凭据', 'Revoke credential')}
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {(!applicationId || !credentials.data?.data.length) && (
          <p className="py-10 text-center text-sm text-[var(--color-text-tertiary)]">
            {!applicationId
              ? text('请选择应用', 'Choose an application')
              : credentials.isPending
                ? text('正在加载…', 'Loading…')
                : text('该应用没有已签发凭据', 'No credentials for this application')}
          </p>
        )}
        <div className="flex justify-between p-4 text-sm">
          <span>{credentials.data?.total ?? '—'}</span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={!offset || credentials.isFetching}
              onClick={() => setOffset(Math.max(0, offset - 20))}
            >
              {text('上一页', 'Previous')}
            </Button>
            <Button
              variant="outline"
              disabled={
                !credentials.data || offset + 20 >= credentials.data.total || credentials.isFetching
              }
              onClick={() => setOffset(offset + 20)}
            >
              {text('下一页', 'Next')}
            </Button>
          </div>
        </div>
      </section>
      <Dialog
        open={!!pending}
        onOpenChange={(open) => {
          if (!open && !busy) {
            setPending(null);
            setError('');
          }
        }}
      >
        <DialogContent>
          <DialogTitle>
            {pending?.kind === 'issue'
              ? text('签发调用凭据', 'Issue credential')
              : text('撤销调用凭据', 'Revoke credential')}
          </DialogTitle>
          <DialogDescription>
            {pending?.kind === 'issue' ? pending.application.name : pending?.credential.id}
          </DialogDescription>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void commit();
            }}
            className="mt-4 space-y-4"
          >
            {pending?.kind === 'issue' && (
              <ExpiryInput
                label={text('凭据到期时间', 'Credential expiry')}
                value={pending.expires_at}
                onChange={(value) =>
                  setPending((current) =>
                    current?.kind === 'issue' ? { ...current, expires_at: value } : current,
                  )
                }
                maximum={String(pending.application.properties.credential_expires_at ?? '')}
                disabled={busy}
                description={text(
                  '留空沿用应用有效期；不能超过应用的到期时间',
                  'Leave empty to use the application expiry; cannot extend it',
                )}
              />
            )}
            {pending?.kind === 'revoke' && (
              <p className="text-sm">
                {text(
                  '撤销后，该凭据将无法继续授权调用 API。',
                  'This credential will no longer authorize API calls.',
                )}
              </p>
            )}
            {error && <ErrorBanner message={error} />}
            <div className="flex justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                disabled={busy}
                onClick={() => setPending(null)}
              >
                {text('取消', 'Cancel')}
              </Button>
              <Button type="submit" disabled={busy}>
                {busy ? text('正在提交…', 'Submitting…') : text('确认', 'Confirm')}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
      <Dialog
        open={!!issued}
        onOpenChange={(open) => {
          if (!open) {
            setIssued(null);
            setCopied(false);
            setError('');
          }
        }}
      >
        <DialogContent>
          <DialogTitle>{text('调用凭据签发结果', 'Credential issuance result')}</DialogTitle>
          <DialogDescription>{issued?.credential.id}</DialogDescription>
          {issued?.secret ? (
            <div className="mt-4 space-y-3">
              <p className="text-sm">
                {text(
                  '请保存这次签发的完整凭据。关闭后无法再次查看；若未能保存，请撤销后重新签发。',
                  'Save the complete credential now. It cannot be retrieved after closing. Revoke and issue a new one if it was not saved.',
                )}
              </p>
              <pre className="whitespace-pre-wrap break-all rounded-md bg-[var(--color-bg-secondary)] p-4 text-sm">
                {issued.secret}
              </pre>
              <Button onClick={() => void copy()}>
                {copied ? text('已复制', 'Copied') : text('复制凭据', 'Copy credential')}
              </Button>
            </div>
          ) : (
            <p className="mt-4 text-sm">
              {text(
                '此前签发请求已成功处理，本次为重试回执。原始凭据不能再次读取；请核对记录，需要时撤销并重新签发。',
                'The earlier request was processed; this is a replay receipt. The original credential cannot be retrieved. Revoke and issue again if needed.',
              )}
            </p>
          )}
          {error && <ErrorBanner message={error} />}
        </DialogContent>
      </Dialog>
    </div>
  );
}
