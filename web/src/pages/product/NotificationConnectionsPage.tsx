import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Plus, RefreshCw, Send, Settings2, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import client from '../../api/client';
import { Button } from '../../components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../../components/ui/dialog';
import { ErrorBanner, LoadingSpinner, StatusBadge } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import { useAuthStore } from '../../stores/auth';

type Channel = 'email' | 'webhook' | 'feishu' | 'dingtalk' | 'wechat_work';
interface Connection {
  id: string; name: string; channel: Channel; public_config: Record<string, unknown>;
  has_secret: boolean; enabled: boolean; version: number;
  last_test_status: 'success' | 'failed' | null; last_test_error: string | null;
  last_test_at: string | null; created_at: string; updated_at: string;
}
interface FormState {
  name: string; channel: Channel; endpoint: string; smtpHost: string; smtpPort: string;
  username: string; from: string; to: string; secret: string; enabled: boolean;
}
const blank: FormState = { name: '', channel: 'webhook', endpoint: '', smtpHost: '', smtpPort: '587', username: '', from: '', to: '', secret: '', enabled: true };
const channelLabels: Record<Channel, [string, string]> = {
  email: ['邮件', 'Email'], webhook: ['通用 Webhook', 'Webhook'], feishu: ['飞书机器人', 'Feishu bot'],
  dingtalk: ['钉钉机器人', 'DingTalk bot'], wechat_work: ['企业微信机器人', 'WeCom bot'],
};

export default function NotificationConnectionsPage() {
  const { i18n } = useTranslation(), zh = i18n.language.startsWith('zh');
  const text = (cn: string, en: string) => zh ? cn : en;
  const permissions = useAuthStore((state) => state.user?.permissions ?? []);
  const canRead = permissions.includes('support:read'), canWrite = permissions.includes('support:write');
  const cache = useQueryClient();
  const [editing, setEditing] = useState<Connection | null | undefined>(undefined);
  const [form, setForm] = useState<FormState>(blank), [actionError, setActionError] = useState('');
  const query = useQuery({
    queryKey: ['notification-connections'],
    queryFn: async () => (await client.get<{ data: Connection[] }>('/notification-connections')).data.data,
    enabled: canRead,
  });
  const save = useMutation({
    mutationFn: async () => {
      const public_config = form.channel === 'email'
        ? { smtp_host: form.smtpHost.trim(), smtp_port: Number(form.smtpPort), username: form.username.trim(), from: form.from.trim(), to: form.to.split(',').map((item) => item.trim()).filter(Boolean) }
        : form.channel === 'webhook' ? { url: form.endpoint.trim() } : { webhook_url: form.endpoint.trim() };
      if (editing) return (await client.put<Connection>('/notification-connections/' + encodeURIComponent(editing.id), {
        expected_version: editing.version, name: form.name, public_config,
        replace_secret: form.secret.length > 0, secret: form.secret || null,
      })).data;
      return (await client.post<Connection>('/notification-connections', {
        name: form.name, channel: form.channel, public_config, secret: form.secret || null, enabled: form.enabled,
      })).data;
    },
    onSuccess: async () => { setEditing(undefined); setForm(blank); setActionError(''); await cache.invalidateQueries({ queryKey: ['notification-connections'] }); },
    onError: (error) => setActionError(apiErrorMessage(error, text('保存连接失败', 'Could not save connection'))),
  });
  const action = useMutation({
    mutationFn: async ({ connection, kind }: { connection: Connection; kind: 'test' | 'toggle' | 'delete' }) => {
      const path = '/notification-connections/' + encodeURIComponent(connection.id);
      if (kind === 'test') return client.post(path + '/test');
      if (kind === 'toggle') return client.patch(path + '/enabled', { expected_version: connection.version, enabled: !connection.enabled });
      return client.delete(path, { params: { expected_version: connection.version } });
    },
    onSuccess: async () => { setActionError(''); await cache.invalidateQueries({ queryKey: ['notification-connections'] }); },
    onError: (error) => setActionError(apiErrorMessage(error, text('连接操作失败', 'Connection action failed'))),
  });
  const openEdit = (connection: Connection) => {
    const config = connection.public_config;
    setForm({
      name: connection.name, channel: connection.channel, endpoint: String(config.url ?? config.webhook_url ?? ''),
      smtpHost: String(config.smtp_host ?? ''), smtpPort: String(config.smtp_port ?? '587'),
      username: String(config.username ?? ''), from: String(config.from ?? ''),
      to: Array.isArray(config.to) ? config.to.join(', ') : '', secret: '', enabled: connection.enabled,
    });
    setEditing(connection); setActionError('');
  };
  if (!canRead) return <ErrorBanner message={text('当前账号没有辅助管理查看权限', 'You cannot read supporting tools')} />;
  const data = query.data ?? [], enabled = data.filter((item) => item.enabled).length;
  const healthy = data.filter((item) => item.last_test_status === 'success').length;

  return <div className="space-y-5">
    <header className="flex flex-col justify-between gap-4 sm:flex-row sm:items-start">
      <div>
        <p className="text-xs font-medium uppercase tracking-[0.18em] text-[var(--color-accent)]">{text('消息基础设施', 'Messaging infrastructure')}</p>
        <h1 className="mt-1 text-2xl font-semibold">{text('通知连接', 'Notification connections')}</h1>
        <p className="mt-2 max-w-3xl text-sm text-[var(--color-text-secondary)]">{text('集中维护告警和系统消息使用的真实渠道连接。凭据加密保存，列表与普通详情不会返回密钥。', 'Manage delivery channels used by alerts and system messages. Secrets are encrypted and never returned by list or detail APIs.')}</p>
      </div>
      <div className="flex gap-2">
        <Button variant="outline" onClick={() => void query.refetch()} disabled={query.isFetching}><RefreshCw className={query.isFetching ? 'animate-spin' : ''} />{text('刷新', 'Refresh')}</Button>
        <Button onClick={() => { setForm(blank); setEditing(null); setActionError(''); }} disabled={!canWrite}><Plus />{text('新建连接', 'New connection')}</Button>
      </div>
    </header>
    <section className="grid gap-3 sm:grid-cols-3">
      {[[text('连接总数', 'Connections'), data.length], [text('已启用', 'Enabled'), enabled], [text('最近测试成功', 'Last test passed'), healthy]].map(([label, value]) =>
        <div key={String(label)} className="data-panel p-4"><p className="text-xs text-[var(--color-text-tertiary)]">{label}</p><p className="mt-2 text-2xl font-semibold">{value}</p></div>)}
    </section>
    {(actionError || query.error) && <ErrorBanner message={actionError || apiErrorMessage(query.error, text('通知连接加载失败', 'Could not load connections'))} />}
    <section className="data-panel overflow-hidden">
      {query.isPending ? <div className="flex justify-center py-14"><LoadingSpinner /></div> :
        <div className="overflow-auto"><table className="data-table"><thead><tr>
          {[text('名称', 'Name'), text('渠道', 'Channel'), text('状态', 'Status'), text('最近测试', 'Last test'), text('凭据', 'Secret'), text('操作', 'Actions')].map((item) => <th key={item}>{item}</th>)}
        </tr></thead><tbody>{data.map((connection) => <tr key={connection.id}>
          <td><p className="font-medium">{connection.name}</p><p className="mt-1 text-xs text-[var(--color-text-tertiary)]">{connection.id}</p></td>
          <td>{zh ? channelLabels[connection.channel][0] : channelLabels[connection.channel][1]}</td>
          <td><StatusBadge status={connection.enabled ? 'active' : 'disabled'} /></td>
          <td><p>{connection.last_test_status === 'success' ? text('成功', 'Passed') : connection.last_test_status === 'failed' ? text('失败', 'Failed') : text('未测试', 'Not tested')}</p>
            {connection.last_test_at && <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">{new Date(connection.last_test_at).toLocaleString(i18n.language)}</p>}
            {connection.last_test_error && <p className="mt-1 max-w-xs text-xs text-red-500">{connection.last_test_error}</p>}</td>
          <td>{connection.has_secret ? text('已安全配置', 'Configured') : text('未配置', 'Missing')}</td>
          <td><div className="flex flex-wrap gap-2">
            <Button size="sm" variant="outline" disabled={!canWrite || action.isPending} onClick={() => action.mutate({ connection, kind: 'test' })}><Send />{text('测试', 'Test')}</Button>
            <Button size="sm" variant="outline" disabled={!canWrite || action.isPending} onClick={() => openEdit(connection)}><Settings2 />{text('编辑', 'Edit')}</Button>
            <Button size="sm" variant="outline" disabled={!canWrite || action.isPending} onClick={() => action.mutate({ connection, kind: 'toggle' })}>{connection.enabled ? text('停用', 'Disable') : text('启用', 'Enable')}</Button>
            <Button size="sm" variant="destructive" disabled={!canWrite || action.isPending} onClick={() => { if (window.confirm(text('确定删除“' + connection.name + '”吗？', 'Delete “' + connection.name + '”?'))) action.mutate({ connection, kind: 'delete' }); }}><Trash2 />{text('删除', 'Delete')}</Button>
          </div></td>
        </tr>)}</tbody></table>{!data.length && <p className="py-14 text-center text-sm text-[var(--color-text-tertiary)]">{text('尚未配置通知连接', 'No notification connections')}</p>}</div>}
    </section>
    <Dialog open={editing !== undefined} onOpenChange={(open) => { if (!open && !save.isPending) setEditing(undefined); }}>
      <DialogContent className="max-w-2xl">
        <DialogTitle>{editing ? text('编辑通知连接', 'Edit notification connection') : text('新建通知连接', 'New notification connection')}</DialogTitle>
        <DialogDescription>{text('密钥留空会在编辑时保留现有值。测试结果会作为审计证据保存。', 'Leave the secret blank while editing to keep it. Test results are audited.')}</DialogDescription>
        <form className="mt-4 space-y-4" onSubmit={(event) => { event.preventDefault(); setActionError(''); save.mutate(); }}>
          <label className="block text-sm">{text('连接名称', 'Name')}<input className="form-input mt-1 w-full" required maxLength={120} value={form.name} onChange={(event) => setForm({ ...form, name: event.target.value })} /></label>
          <label className="block text-sm">{text('渠道', 'Channel')}<select className="form-input mt-1 w-full" disabled={!!editing} value={form.channel} onChange={(event) => setForm({ ...form, channel: event.target.value as Channel })}>{Object.entries(channelLabels).map(([value, labels]) => <option key={value} value={value}>{zh ? labels[0] : labels[1]}</option>)}</select></label>
          {form.channel === 'email' ? <div className="grid gap-4 sm:grid-cols-2">
            <Field label={text('SMTP 主机', 'SMTP host')} value={form.smtpHost} onChange={(smtpHost) => setForm({ ...form, smtpHost })} />
            <label className="text-sm">{text('SMTP 端口', 'SMTP port')}<input className="form-input mt-1 w-full" required type="number" min="1" max="65535" value={form.smtpPort} onChange={(event) => setForm({ ...form, smtpPort: event.target.value })} /></label>
            <Field label={text('用户名', 'Username')} value={form.username} onChange={(username) => setForm({ ...form, username })} />
            <label className="text-sm">{text('发件地址', 'From')}<input className="form-input mt-1 w-full" required type="email" value={form.from} onChange={(event) => setForm({ ...form, from: event.target.value })} /></label>
            <div className="sm:col-span-2"><Field label={text('收件地址（逗号分隔）', 'Recipients (comma separated)')} value={form.to} onChange={(to) => setForm({ ...form, to })} /></div>
          </div> : <label className="block text-sm">{text('Webhook 地址', 'Webhook URL')}<input className="form-input mt-1 w-full" required type="url" value={form.endpoint} onChange={(event) => setForm({ ...form, endpoint: event.target.value })} /></label>}
          <label className="block text-sm">{form.channel === 'email' ? text('SMTP 密码', 'SMTP password') : text('签名密钥 / Bot Key', 'Signing secret / bot key')}<input className="form-input mt-1 w-full" type="password" required={!editing && ['email', 'wechat_work'].includes(form.channel)} autoComplete="new-password" value={form.secret} onChange={(event) => setForm({ ...form, secret: event.target.value })} /></label>
          {!editing && <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={form.enabled} onChange={(event) => setForm({ ...form, enabled: event.target.checked })} />{text('创建后立即启用', 'Enable after creation')}</label>}
          {actionError && <ErrorBanner message={actionError} />}
          <div className="flex justify-end gap-2"><Button type="button" variant="outline" disabled={save.isPending} onClick={() => setEditing(undefined)}>{text('取消', 'Cancel')}</Button><Button type="submit" disabled={!canWrite || save.isPending}>{save.isPending ? text('保存中…', 'Saving…') : text('保存', 'Save')}</Button></div>
        </form>
      </DialogContent>
    </Dialog>
  </div>;
}

function Field({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return <label className="block text-sm">{label}<input className="form-input mt-1 w-full" required value={value} onChange={(event) => onChange(event.target.value)} /></label>;
}
