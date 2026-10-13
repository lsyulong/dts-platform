import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ArrowRight, Eye, EyeOff, Languages, LoaderCircle, Moon, Sun } from 'lucide-react';
import { useAuthStore } from '../stores/auth';
import { useAppStore } from '../stores/app';
import { ErrorBanner, FormInput } from '../components/ui';
import { loginSchema, getFirstError } from '../lib/validations';
import LoginBrandPanel from './login/LoginBrandPanel';
import './login/login.css';

function LoginAppearanceControls() {
  const { t, i18n } = useTranslation();
  const { theme, toggleTheme } = useAppStore();
  const isChinese = i18n.language.startsWith('zh');

  return (
    <div className="login-appearance-controls">
      <button
        type="button"
        onClick={() => void i18n.changeLanguage(isChinese ? 'en-US' : 'zh-CN')}
        aria-label={t('language.switch')}
        className="login-appearance-button"
      >
        <Languages size={16} aria-hidden="true" />
        <span>{isChinese ? 'English' : '中文'}</span>
      </button>
      <button
        type="button"
        onClick={toggleTheme}
        aria-label={theme === 'dark' ? t('theme.light') : t('theme.dark')}
        className="login-appearance-button login-theme-button"
      >
        {theme === 'dark' ? <Sun size={17} /> : <Moon size={17} />}
      </button>
    </div>
  );
}

export default function LoginPage() {
  const { t } = useTranslation();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [tenantSlug, setTenantSlug] = useState('default');
  const [passwordVisible, setPasswordVisible] = useState(false);
  const [error, setError] = useState('');
  const { login, loading } = useAuthStore();
  const navigate = useNavigate();

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (loading) return;
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
    <main className="login-page">
      <div className="login-shell">
        <LoginBrandPanel />
        <section className="login-form-panel" aria-labelledby="login-heading">
          <LoginAppearanceControls />
          <div className="login-form-content">
            <div className="login-form-heading">
              <span className="login-form-eyebrow">{t('auth.console')}</span>
              <h2 id="login-heading">{t('auth.welcomeBack')}</h2>
              <p>{t('auth.signInDescription')}</p>
            </div>

            <form onSubmit={handleSubmit} className="login-form" aria-busy={loading}>
              <FormInput
                label={t('auth.tenantSlug')}
                value={tenantSlug}
                onChange={setTenantSlug}
                placeholder="default"
                disabled={loading}
                className="login-input"
              />
              <FormInput
                label={t('auth.username')}
                value={username}
                onChange={setUsername}
                placeholder="admin"
                disabled={loading}
                className="login-input"
              />
              <div className="login-password-field">
                <FormInput
                  label={t('auth.password')}
                  type={passwordVisible ? 'text' : 'password'}
                  value={password}
                  onChange={setPassword}
                  placeholder="••••••••"
                  disabled={loading}
                  className="login-input login-password-input"
                />
                <button
                  type="button"
                  onClick={() => setPasswordVisible((visible) => !visible)}
                  disabled={loading}
                  aria-label={t(passwordVisible ? 'auth.hidePassword' : 'auth.showPassword')}
                  aria-pressed={passwordVisible}
                  className="login-password-toggle"
                >
                  {passwordVisible ? <EyeOff size={18} /> : <Eye size={18} />}
                </button>
              </div>

              {error && <ErrorBanner message={error} />}

              <button type="submit" disabled={loading} className="login-submit">
                <span>{loading ? t('auth.loggingIn') : t('auth.login')}</span>
                {loading ? (
                  <LoaderCircle size={18} className="login-spinner" aria-hidden="true" />
                ) : (
                  <ArrowRight size={18} aria-hidden="true" />
                )}
              </button>
            </form>

            <p className="login-account-hint">{t('auth.defaultCreds')}</p>
          </div>
          <p className="login-form-footer">{t('auth.workspaceFooter')}</p>
        </section>
      </div>
    </main>
  );
}
