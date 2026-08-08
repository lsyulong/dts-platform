import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '../stores/auth';
import { Database, Plug, Zap, Brain, Box } from 'lucide-react';
import { ErrorBanner, FormInput } from '../components/ui';
import { loginSchema, getFirstError } from '../lib/validations';

export default function LoginPage() {
  const { t } = useTranslation();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [tenantSlug, setTenantSlug] = useState('default');
  const [error, setError] = useState('');
  const { login, loading } = useAuthStore();
  const navigate = useNavigate();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    const validation = loginSchema.safeParse({ tenantSlug, username, password });
    if (!validation.success) {
      setError(getFirstError(validation) || t('errors.somethingWentWrong'));
      return;
    }

    try {
      await login(username, password, tenantSlug);
      navigate('/');
    } catch (err: unknown) {
      if (err && typeof err === 'object' && 'response' in err) {
        const axiosErr = err as { response?: { data?: { message?: string } } };
        setError(axiosErr.response?.data?.message || t('auth.loginFailed'));
      } else {
        setError(err instanceof Error ? err.message : t('auth.loginFailed'));
      }
    }
  };

  return (
    <div className="flex min-h-screen items-center justify-center bg-[var(--color-bg-primary)]">
      {/* Background gradient */}
      <div className="pointer-events-none fixed inset-0 bg-[radial-gradient(ellipse_at_top,rgba(37,99,235,0.08),transparent_50%)]" />

      <div className="relative w-full max-w-md rounded-2xl border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] p-8 backdrop-blur-xl">
        {/* Logo */}
        <div className="mb-8 text-center">
          <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-blue-500/10">
            <Database className="text-blue-400" size={28} />
          </div>
          <h1 className="text-2xl font-bold text-[var(--color-text-primary)]">{t('common.appName')}</h1>
          <p className="mt-2 text-sm text-[var(--color-text-tertiary)]">{t('common.appDesc')}</p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <FormInput
            label={t('auth.tenantSlug')}
            value={tenantSlug}
            onChange={(v) => setTenantSlug(v)}
            placeholder="default"
          />
          <FormInput
            label={t('auth.username')}
            value={username}
            onChange={(v) => setUsername(v)}
            placeholder="admin"
          />
          <FormInput
            label={t('auth.password')}
            type="password"
            value={password}
            onChange={(v) => setPassword(v)}
            placeholder="••••••••"
          />

          {error && <ErrorBanner message={error} />}

          <button
            type="submit"
            disabled={loading}
            className="w-full rounded-xl bg-blue-500 px-4 py-2.5 text-sm font-medium text-white shadow-[0_0_20px_rgba(37,99,235,0.2)] transition-all hover:bg-blue-600 disabled:opacity-50"
          >
            {loading ? t('auth.loggingIn') : t('auth.login')}
          </button>
        </form>

        {/* Feature highlights */}
        <div className="mt-6 grid grid-cols-2 gap-2">
          {([
            { key: 'integration', icon: Plug, color: 'text-emerald-400', bg: 'bg-emerald-500/10' },
            { key: 'analysis', icon: Zap, color: 'text-amber-400', bg: 'bg-amber-500/10' },
            { key: 'ai', icon: Brain, color: 'text-violet-400', bg: 'bg-violet-500/10' },
            { key: 'deploy', icon: Box, color: 'text-sky-400', bg: 'bg-sky-500/10' },
          ] as const).map(({ key, icon: Icon, color, bg }) => (
            <div
              key={key}
              className={`flex flex-col items-center gap-1 rounded-xl ${bg} p-3 text-center`}
            >
              <Icon size={18} className={color} />
              <span className="text-xs font-medium text-[var(--color-text-primary)]">
                {t(`auth.loginFeatures.${key}.title`)}
              </span>
              <span className="text-[10px] leading-tight text-[var(--color-text-disabled)]">
                {t(`auth.loginFeatures.${key}.desc`)}
              </span>
            </div>
          ))}
        </div>

        <p className="mt-6 text-center text-xs text-[var(--color-text-disabled)]">
          {t('auth.defaultCreds')}
        </p>
      </div>
    </div>
  );
}
