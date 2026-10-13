import { useId, useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Building2, KeyRound, Save, ShieldCheck, UserRound } from 'lucide-react';
import client from '../../api/client';
import { Button, EmptyState, ErrorBanner } from '../../components/ui';
import { apiErrorMessage } from '../../lib/apiError';
import { captureSession, clearSession, isCurrentSession } from '../../lib/session';
import { useAuthStore } from '../../stores/auth';

type UserInfo = NonNullable<ReturnType<typeof useAuthStore.getState>['user']>;
type WireUserInfo = Omit<UserInfo, 'display_name'> & { display_name?: string | null };
type Text = (zh: string, en: string) => string;

function isUserInfo(value: unknown): value is WireUserInfo {
  if (typeof value !== 'object' || value === null) return false;
  return (
    'id' in value &&
    typeof value.id === 'string' &&
    'username' in value &&
    typeof value.username === 'string' &&
    'email' in value &&
    typeof value.email === 'string' &&
    'tenant_id' in value &&
    typeof value.tenant_id === 'string' &&
    'tenant_name' in value &&
    typeof value.tenant_name === 'string' &&
    'roles' in value &&
    Array.isArray(value.roles) &&
    value.roles.every((role) => typeof role === 'string') &&
    'permissions' in value &&
    Array.isArray(value.permissions) &&
    value.permissions.every((permission) => typeof permission === 'string') &&
    (!('display_name' in value) ||
      value.display_name === undefined ||
      value.display_name === null ||
      typeof value.display_name === 'string')
  );
}

function ProfileField({
  label,
  value,
  onChange,
  type = 'text',
  autoComplete,
  disabled,
  required = true,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  type?: 'text' | 'email' | 'password';
  autoComplete: string;
  disabled: boolean;
  required?: boolean;
}) {
  const id = useId();
  return (
    <div>
      <label
        htmlFor={id}
        className="mb-1.5 block text-sm font-medium text-[var(--color-text-secondary)]"
      >
        {label}
      </label>
      <input
        id={id}
        type={type}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        autoComplete={autoComplete}
        disabled={disabled}
        required={required}
        className="w-full rounded-md border border-[var(--color-border-hover)] bg-[var(--color-bg-elevated)] px-3 py-2 text-sm text-[var(--color-text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--color-accent)] disabled:opacity-50"
      />
    </div>
  );
}

function ProfileEditor({ user, text }: { user: UserInfo; text: Text }) {
  const [displayName, setDisplayName] = useState(user.display_name ?? '');
  const [email, setEmail] = useState(user.email);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [success, setSuccess] = useState('');
  const changed = displayName !== (user.display_name ?? '') || email !== user.email;

  const save = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const session = captureSession();
    if (!session.token || saving) return;
    if (!displayName.trim() || !email.trim()) {
      setError(text('显示名称和邮箱不能为空', 'Display name and email are required'));
      return;
    }
    setSaving(true);
    setError('');
    setSuccess('');
    try {
      const response = await client.patch<unknown>(
        '/auth/profile',
        { email: email.trim(), display_name: displayName.trim() },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (!isCurrentSession(session)) return;
      const actual = response.data;
      if (
        !isUserInfo(actual) ||
        actual.id !== session.userId ||
        actual.tenant_id !== session.tenantId
      )
        throw new Error(
          text('服务器返回的账户信息不正确', 'The server returned invalid account information'),
        );
      const updated: UserInfo = { ...actual, display_name: actual.display_name ?? undefined };
      let cached = true;
      try {
        localStorage.setItem('user', JSON.stringify(updated));
      } catch {
        cached = false;
      }
      useAuthStore.setState({ user: updated });
      setDisplayName(updated.display_name ?? '');
      setEmail(updated.email);
      setSuccess(
        cached
          ? text('基本资料已保存', 'Your profile has been saved')
          : text(
              '资料已保存；本地缓存不可用，刷新后将重新读取服务器资料。',
              'Your profile was saved. Local caching is unavailable; reloading will restore it from the server.',
            ),
      );
    } catch (failure) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(failure, text('资料保存失败', 'Failed to save your profile')));
    } finally {
      if (isCurrentSession(session)) setSaving(false);
    }
  };

  return (
    <form onSubmit={(event) => void save(event)} className="space-y-5">
      {error && (
        <div role="alert">
          <ErrorBanner message={error} />
        </div>
      )}
      {success && (
        <p
          role="status"
          className="rounded-md bg-[var(--color-accent-subtle)] px-3 py-2 text-sm text-[var(--color-accent)]"
        >
          {success}
        </p>
      )}
      <div className="grid gap-4 sm:grid-cols-2">
        <ProfileField
          label={text('显示名称', 'Display name')}
          value={displayName}
          onChange={(value) => {
            setDisplayName(value);
            setSuccess('');
          }}
          autoComplete="nickname"
          disabled={saving}
        />
        <ProfileField
          label={text('电子邮箱', 'Email')}
          value={email}
          onChange={(value) => {
            setEmail(value);
            setSuccess('');
          }}
          type="email"
          autoComplete="email"
          disabled={saving}
        />
      </div>
      <div className="flex justify-end">
        <Button type="submit" disabled={saving || !changed}>
          <Save size={16} />
          {text(saving ? '保存中…' : '保存基本资料', saving ? 'Saving…' : 'Save profile')}
        </Button>
      </div>
    </form>
  );
}

function PasswordEditor({ text }: { text: Text }) {
  const navigate = useNavigate();
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const change = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const session = captureSession();
    if (!session.token || saving) return;
    if (newPassword !== confirmation) {
      setError(text('两次输入的新密码不一致', 'The new passwords do not match'));
      return;
    }
    if (newPassword === currentPassword) {
      setError(
        text('新密码必须与当前密码不同', 'Choose a password different from the current password'),
      );
      return;
    }
    if (new TextEncoder().encode(newPassword).length < 12) {
      setError(text('新密码至少需要 12 字节', 'The new password must contain at least 12 bytes'));
      return;
    }
    setSaving(true);
    setError('');
    try {
      const response = await client.post(
        '/auth/change-password',
        { current_password: currentPassword, new_password: newPassword },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (!isCurrentSession(session)) return;
      if (response.status !== 204)
        throw new Error(
          text('服务器未确认密码变更完成', 'The server did not confirm the password change'),
        );
      setCurrentPassword('');
      setNewPassword('');
      setConfirmation('');
      if (clearSession(session.token)) navigate('/login', { replace: true });
    } catch (failure) {
      if (isCurrentSession(session))
        setError(apiErrorMessage(failure, text('密码修改失败', 'Failed to change your password')));
    } finally {
      if (isCurrentSession(session)) setSaving(false);
    }
  };

  return (
    <form onSubmit={(event) => void change(event)} className="space-y-5">
      {error && (
        <div role="alert">
          <ErrorBanner message={error} />
        </div>
      )}
      <p className="text-sm text-[var(--color-text-tertiary)]">
        {text(
          '修改密码需要验证当前密码。成功后将退出登录，请使用新密码重新登录。',
          'Verify your current password to change it. After success, sign in again with your new password.',
        )}
      </p>
      <div className="grid gap-4 sm:grid-cols-3">
        <ProfileField
          label={text('当前密码', 'Current password')}
          value={currentPassword}
          onChange={setCurrentPassword}
          type="password"
          autoComplete="current-password"
          disabled={saving}
        />
        <ProfileField
          label={text('新密码', 'New password')}
          value={newPassword}
          onChange={setNewPassword}
          type="password"
          autoComplete="new-password"
          disabled={saving}
        />
        <ProfileField
          label={text('确认新密码', 'Confirm new password')}
          value={confirmation}
          onChange={setConfirmation}
          type="password"
          autoComplete="new-password"
          disabled={saving}
        />
      </div>
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {text(
          '新密码至少 12 字节；密码只发送到认证接口，不保存在本地缓存。',
          'Use at least 12 bytes. Passwords are sent only to the authentication endpoint and are not stored locally.',
        )}
      </p>
      <div className="flex justify-end">
        <Button
          type="submit"
          disabled={saving || !currentPassword || !newPassword || !confirmation}
        >
          <KeyRound size={16} />
          {text(
            saving ? '修改中…' : '修改密码并重新登录',
            saving ? 'Changing…' : 'Change password and sign in again',
          )}
        </Button>
      </div>
    </form>
  );
}

export default function ProfilePage() {
  const { i18n } = useTranslation();
  const text: Text = (zh, en) => (i18n.language.startsWith('zh') ? zh : en);
  const user = useAuthStore((state) => state.user);
  const [tab, setTab] = useState<'profile' | 'security'>('profile');
  if (!user)
    return (
      <EmptyState
        icon={UserRound}
        title={text('请先登录以查看个人资料', 'Sign in to view your profile')}
      />
    );
  const editorKey = `${user.tenant_id}:${user.id}`;

  return (
    <div className="space-y-6">
      <div>
        <h1 className="flex items-center gap-2 text-xl font-semibold text-[var(--color-text-primary)]">
          <UserRound size={22} />
          {text('个人中心', 'Personal center')}
        </h1>
        <p className="mt-1 text-sm text-[var(--color-text-tertiary)]">
          {text('维护本人资料与账户安全', 'Manage your own profile and account security')}
        </p>
      </div>
      <section className="data-panel p-4 sm:p-5" aria-label={text('当前账户', 'Current account')}>
        <div className="flex flex-wrap items-start gap-4">
          <div className="flex size-12 items-center justify-center rounded-xl bg-[var(--color-accent-subtle)] text-[var(--color-accent)]">
            <UserRound size={26} />
          </div>
          <div className="min-w-0 flex-1">
            <h2 className="text-base font-semibold text-[var(--color-text-primary)]">
              {user.display_name || user.username}
            </h2>
            <p className="mt-1 break-all text-sm text-[var(--color-text-tertiary)]">
              {text('账号', 'Username')}: {user.username}
            </p>
          </div>
          <div className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
            <Building2 size={16} />
            {user.tenant_name}
            <span className="break-all text-xs text-[var(--color-text-tertiary)]">
              ({user.tenant_id})
            </span>
          </div>
        </div>
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <span className="text-sm text-[var(--color-text-tertiary)]">
            {text('当前角色', 'Assigned roles')}:
          </span>
          {user.roles.length ? (
            user.roles.map((role) => (
              <span
                key={role}
                className="rounded-md bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-secondary)]"
              >
                {role}
              </span>
            ))
          ) : (
            <span className="text-sm text-[var(--color-text-tertiary)]">
              {text('未分配角色', 'No assigned roles')}
            </span>
          )}
        </div>
      </section>
      <section className="data-panel">
        <div
          role="tablist"
          aria-label={text('个人中心设置', 'Personal settings')}
          className="flex gap-2 border-b border-[var(--color-border-default)] px-3"
        >
          {(['profile', 'security'] as const).map((key) => (
            <button
              key={key}
              id={`profile-tab-${key}`}
              role="tab"
              type="button"
              tabIndex={tab === key ? 0 : -1}
              aria-selected={tab === key}
              aria-controls={`profile-panel-${key}`}
              onClick={() => setTab(key)}
              onKeyDown={(event) => {
                if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
                event.preventDefault();
                const next =
                  event.key === 'Home'
                    ? 'profile'
                    : event.key === 'End'
                      ? 'security'
                      : key === 'profile'
                        ? 'security'
                        : 'profile';
                setTab(next);
                document.getElementById(`profile-tab-${next}`)?.focus();
              }}
              className={`flex items-center gap-2 border-b-2 px-4 py-3 text-sm font-medium focus-visible:outline focus-visible:outline-[var(--color-accent)] ${tab === key ? 'border-[var(--color-accent)] text-[var(--color-accent)]' : 'border-transparent text-[var(--color-text-tertiary)]'}`}
            >
              {key === 'profile' ? <UserRound size={16} /> : <ShieldCheck size={16} />}
              {key === 'profile' ? text('基本资料', 'Profile') : text('账户安全', 'Security')}
            </button>
          ))}
        </div>
        <div
          id={`profile-panel-${tab}`}
          role="tabpanel"
          aria-labelledby={`profile-tab-${tab}`}
          className="p-4 sm:p-5"
        >
          {tab === 'profile' ? (
            <ProfileEditor key={editorKey} user={user} text={text} />
          ) : (
            <PasswordEditor key={editorKey} text={text} />
          )}
        </div>
      </section>
    </div>
  );
}
