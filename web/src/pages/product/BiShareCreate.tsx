import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../components/ui/dialog';
import { useAuthStore } from '../../stores/auth';
import ReferencePicker from '../../product/ReferencePicker';
import ExpiryInput from '../../product/ExpiryInput';
import { captureSession, isCurrentSession } from '../../lib/session';
import { apiErrorMessage } from '../../lib/apiError';
import type { BiShare } from './BiSharesPage';

export default function BiShareCreate({
  reportId,
  version,
  jobId,
  nodeId,
}: {
  reportId: string;
  version: number;
  jobId: string;
  nodeId: string;
}) {
  const { i18n } = useTranslation(),
    canWrite = useAuthStore((state) => state.user?.permissions.includes('bi:write'));
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [open, setOpen] = useState(false),
    [recipient, setRecipient] = useState(''),
    [expires, setExpires] = useState<string | undefined>(() =>
      new Date(Date.now() + 7 * 86400000).toISOString(),
    );
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(''),
    [share, setShare] = useState<BiShare | null>(null);
  const cache = useQueryClient();
  const create = async () => {
    if (!canWrite || busy || !recipient || !expires) return;
    const session = captureSession();
    setBusy(true);
    setError('');
    try {
      const response = await client.post<BiShare>(
        `/product/resources/bi_report/${encodeURIComponent(reportId)}/shares`,
        {
          expected_version: version,
          job_id: jobId,
          node_id: nodeId,
          recipient_id: recipient,
          expires_at: expires,
        },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (isCurrentSession(session)) {
        setShare(response.data);
        void cache.invalidateQueries({ queryKey: ['bi-shares'] });
      }
    } catch (caught) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(caught, text('报表分享失败', 'Could not share report')));
    } finally {
      if (isCurrentSession(session)) setBusy(false);
    }
  };
  if (!canWrite) return null;
  return (
    <>
      <Button
        className="mt-3"
        variant="outline"
        size="sm"
        onClick={() => {
          setShare(null);
          setError('');
          setOpen(true);
        }}
      >
        {text('分享本次 BI 结果', 'Share this BI result')}
      </Button>
      <Dialog
        open={open}
        onOpenChange={(value) => {
          if (!busy) {
            setOpen(value);
            setShare(null);
            setError('');
          }
        }}
      >
        <DialogContent>
          <DialogTitle>{text('受控报表分享', 'Controlled report sharing')}</DialogTitle>
          <DialogDescription>
            {text(
              '分享固定运行和报表版本，接收人登录后仍须具有当前数据访问授权。',
              'Share a fixed run and report version. The recipient must log in and retain current data access.',
            )}
          </DialogDescription>
          {share ? (
            <div className="mt-4 space-y-3">
              <p role="status" className="text-sm text-[var(--color-accent)]">
                {text('分享已保存', 'Share saved')}
              </p>
              <p className="break-all text-xs">{share.id}</p>
              <Link
                className="text-sm text-[var(--color-accent)] hover:underline"
                to={`/bi/personnel?share=${encodeURIComponent(share.id)}`}
                onClick={() => setOpen(false)}
              >
                {text('查看分享记录', 'View share')}
              </Link>
            </div>
          ) : (
            <form
              className="mt-4 space-y-4"
              onSubmit={(event) => {
                event.preventDefault();
                void create();
              }}
            >
              <ReferencePicker
                kind="user"
                resourceKind="bi_report"
                label={text('接收用户', 'Recipient')}
                value={recipient}
                onChange={setRecipient}
                required
                disabled={busy}
              />
              <ExpiryInput
                label={text('分享到期时间', 'Share expiry')}
                value={expires}
                onChange={setExpires}
                disabled={busy}
                maximum={new Date(Date.now() + 365 * 86400000).toISOString()}
              />
              {error && <ErrorBanner message={error} />}
              <div className="flex justify-end gap-2">
                <Button
                  type="button"
                  variant="outline"
                  disabled={busy}
                  onClick={() => setOpen(false)}
                >
                  {text('取消', 'Cancel')}
                </Button>
                <Button type="submit" disabled={busy || !recipient || !expires}>
                  {busy ? text('正在保存…', 'Saving…') : text('保存分享', 'Save share')}
                </Button>
              </div>
            </form>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
