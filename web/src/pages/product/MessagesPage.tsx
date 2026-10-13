import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Mail, RefreshCw, CheckCheck } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { ErrorBanner } from '../../components/ui';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../components/ui/dialog';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, isCurrentSession } from '../../lib/session';
import { useAuthStore } from '../../stores/auth';
import type { ProductModule } from '../../product/catalog-types';
import ProductModulePage from '../../product/ProductModulePage';
import { useMarkMessageRead, useMessageInbox, type MessageDelivery } from './messages-api';
import { QualityNotices } from './QualityMonitorPage';

function MessageInbox() {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [unreadOnly, setUnreadOnly] = useState(false);
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<MessageDelivery | null>(null);
  const [error, setError] = useState('');
  const inbox = useMessageInbox(unreadOnly, offset);
  const mutation = useMarkMessageRead();
  const markRead = async (message: MessageDelivery) => {
    const session = captureSession();
    setError('');
    try {
      const result = await mutation.mutateAsync(message.id);
      if (isCurrentSession(session))
        setSelected((current) => (current?.id === message.id ? result.message : current));
    } catch (caught) {
      if (isCurrentSession(session))
        setError(
          apiErrorMessage(caught, text('更新已读状态失败', 'Could not mark the message read')),
        );
    }
  };

  return (
    <section className="data-panel">
      <div className="data-toolbar">
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={unreadOnly}
            onChange={(event) => {
              setUnreadOnly(event.target.checked);
              setOffset(0);
            }}
          />
          {text('只看未读', 'Unread only')}
        </label>
        <Button
          variant="ghost"
          aria-label={text('刷新收件箱', 'Refresh inbox')}
          disabled={inbox.isFetching}
          onClick={() => void inbox.refetch()}
        >
          <RefreshCw size={16} />
        </Button>
      </div>
      {(error || inbox.error) && (
        <div className="p-4">
          <ErrorBanner
            message={
              error || apiErrorMessage(inbox.error, text('收件箱加载失败', 'Could not load inbox'))
            }
          />
        </div>
      )}
      <div className="divide-y divide-[var(--color-border-default)]">
        {!inbox.error &&
          inbox.data?.data.map((message) => (
            <div key={message.id} className="flex items-center gap-3 px-5 py-4">
              <Mail
                size={20}
                className={
                  message.read_at
                    ? 'text-[var(--color-text-tertiary)]'
                    : 'text-[var(--color-accent)]'
                }
              />
              <button className="min-w-0 flex-1 text-left" onClick={() => setSelected(message)}>
                <div className={`truncate text-sm ${message.read_at ? '' : 'font-semibold'}`}>
                  {message.subject}
                </div>
                <div className="mt-1 text-xs text-[var(--color-accent)]">
                  {message.source_kind === 'alert_policy'
                    ? text('策略告警', 'Policy alert')
                    : text('系统消息', 'System message')}
                </div>
                <div className="mt-1 truncate text-xs text-[var(--color-text-tertiary)]">
                  {message.body}
                </div>
                <time
                  className="mt-1 block text-xs text-[var(--color-text-tertiary)]"
                  dateTime={message.delivered_at}
                >
                  {new Date(message.delivered_at).toLocaleString(i18n.language)}
                </time>
              </button>
              <span className="text-xs text-[var(--color-text-tertiary)]">
                {message.read_at ? text('已读', 'Read') : text('未读', 'Unread')}
              </span>
              {!message.read_at && (
                <Button
                  variant="outline"
                  disabled={mutation.isPending}
                  onClick={() => void markRead(message)}
                >
                  <CheckCheck size={14} />
                  {text('标为已读', 'Mark read')}
                </Button>
              )}
            </div>
          ))}
      </div>
      {!inbox.error && !inbox.isLoading && !inbox.data?.data.length && (
        <div className="px-5 py-14 text-center text-sm text-[var(--color-text-tertiary)]">
          {inbox.isPending
            ? text('正在加载', 'Loading')
            : text('没有收到符合筛选条件的消息', 'No messages match this filter')}
        </div>
      )}
      <div className="flex items-center justify-between border-t border-[var(--color-border-default)] px-5 py-3 text-sm">
        <span>
          {!inbox.error && inbox.data ? `${inbox.data.total} ${text('条消息', 'messages')}` : '—'}
        </span>
        <div className="flex gap-2">
          <Button
            variant="outline"
            disabled={!offset || inbox.isFetching}
            onClick={() => setOffset(Math.max(0, offset - 20))}
          >
            {text('上一页', 'Previous')}
          </Button>
          <Button
            variant="outline"
            disabled={
              !!inbox.error || !inbox.data || offset + 20 >= inbox.data.total || inbox.isFetching
            }
            onClick={() => setOffset(offset + 20)}
          >
            {text('下一页', 'Next')}
          </Button>
        </div>
      </div>
      <Dialog
        open={!inbox.error && !!selected}
        onOpenChange={(open) => {
          if (!open) setSelected(null);
        }}
      >
        <DialogContent>
          <DialogTitle>{selected?.subject}</DialogTitle>
          <DialogDescription>
            {selected ? new Date(selected.delivered_at).toLocaleString(i18n.language) : ''}
          </DialogDescription>
          <p className="mt-4 whitespace-pre-wrap break-words text-sm">{selected?.body}</p>
          {selected && !selected.read_at && (
            <Button
              className="mt-4"
              disabled={mutation.isPending}
              onClick={() => void markRead(selected)}
            >
              {text('标为已读', 'Mark read')}
            </Button>
          )}
          {error && <ErrorBanner message={error} />}
        </DialogContent>
      </Dialog>
    </section>
  );
}

export default function MessagesPage({ module }: { module: ProductModule }) {
  const identity = useAuthStore((state) =>
    JSON.stringify([state.user?.tenant_id, state.user?.id, state.token, state.user?.permissions]),
  );
  return <MessagesContent key={identity} module={module} />;
}

function MessagesContent({ module }: { module: ProductModule }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const canManage = useAuthStore((state) => state.user?.permissions.includes('system:read'));
  const [tab, setTab] = useState<'inbox' | 'management' | 'quality'>('inbox');
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">{text('消息中心', 'Message center')}</h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {text(
            '查看发送给本人的站内消息及实际投递状态',
            'View your inbox and persisted delivery status',
          )}
        </p>
      </div>
      <div className="flex gap-2">
        <Button variant={tab === 'inbox' ? 'default' : 'outline'} onClick={() => setTab('inbox')}>
          {text('我的收件箱', 'My inbox')}
        </Button>
        <Button
          variant={tab === 'quality' ? 'default' : 'outline'}
          onClick={() => setTab('quality')}
        >
          {text('质量提醒', 'Quality reminders')}
        </Button>
        {canManage && (
          <Button
            variant={tab === 'management' ? 'default' : 'outline'}
            onClick={() => setTab('management')}
          >
            {text('消息发送管理', 'Delivery management')}
          </Button>
        )}
      </div>
      {tab === 'management' && canManage ? (
        <ProductModulePage module={module} />
      ) : tab === 'quality' ? (
        <QualityNotices />
      ) : (
        <MessageInbox />
      )}
    </div>
  );
}
