import { Link, useLocation, useNavigate } from 'react-router-dom';
import { useState } from 'react';
import { apiErrorMessage } from '../lib/apiError';
import { LogOut, Sun, Moon, Languages, Menu, ChevronRight, Building2, Bell } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '../stores/auth';
import { useAppStore } from '../stores/app';
import { getProductModuleByPath, productGroups } from '../product/catalog';
import { localized } from '../product/ui-text';

export default function Header() {
  const { t, i18n } = useTranslation();
  const { user, logout } = useAuthStore();
  const { theme, toggleTheme, toggleMobileSidebar } = useAppStore();
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const productModule = getProductModuleByPath(pathname);
  const productGroup = productGroups.find((group) => group.id === productModule?.groupId);
  const routeLabel =
    pathname === '/tasks/create'
      ? t('tasks.createTitle')
      : pathname.startsWith('/tasks/')
        ? t('tasks.detail.title')
        : productModule &&
            ![
              '/',
              '/tasks',
              '/datasources',
              '/query',
              '/analysis',
              '/monitoring',
              '/ops',
              '/system',
            ].includes(pathname)
          ? localized(productModule.title, i18n.language)
          : t(
              (
                {
                  '/tasks': 'nav.tasks',
                  '/datasources': 'nav.datasources',
                  '/monitoring': 'nav.monitoring',
                  '/ops': 'nav.ops',
                  '/system': 'nav.system',
                  '/query': 'nav.query',
                  '/analysis': 'nav.analysis',
                } as Record<string, string>
              )[pathname] ?? 'nav.dashboard',
            );

  const [loggingOut, setLoggingOut] = useState(false);
  const [logoutError, setLogoutError] = useState<string | null>(null);
  const handleLogout = async () => {
    if (loggingOut) return;
    setLoggingOut(true);
    setLogoutError(null);
    try {
      await logout();
      if (!useAuthStore.getState().token) navigate('/login');
    } catch (error) {
      setLogoutError(apiErrorMessage(error, t('auth.logoutFailed')));
    } finally {
      setLoggingOut(false);
    }
  };

  const toggleLanguage = () => {
    const next = i18n.language === 'zh-CN' ? 'en-US' : 'zh-CN';
    i18n.changeLanguage(next);
  };

  return (
    <header className="flex min-h-16 shrink-0 flex-wrap items-center justify-between gap-x-3 gap-y-2 border-b border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] px-3 py-3 sm:px-5 lg:px-6">
      <div className="flex min-w-0 flex-1 items-center gap-3">
        <button
          onClick={toggleMobileSidebar}
          className="rounded-lg p-1.5 text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] lg:hidden"
          aria-label={t('nav.toggleMenu')}
        >
          <Menu size={20} />
        </button>
        <nav aria-label={t('nav.breadcrumb')} className="flex min-w-0 items-center gap-2 text-sm">
          {pathname === '/' ? (
            <span aria-current="page">{t('nav.dashboard')}</span>
          ) : (
            <>
              <Link
                to="/"
                className="hidden text-[var(--color-text-tertiary)] hover:text-[var(--color-accent)] sm:inline"
              >
                {t('nav.dashboard')}
              </Link>
              <ChevronRight
                size={14}
                className="hidden shrink-0 text-[var(--color-text-tertiary)] sm:block"
              />
              {pathname.startsWith('/tasks/') && (
                <>
                  <Link
                    to="/tasks"
                    className="hidden text-[var(--color-text-tertiary)] hover:text-[var(--color-accent)] md:inline"
                  >
                    {t('nav.tasks')}
                  </Link>
                  <ChevronRight
                    size={14}
                    className="hidden shrink-0 text-[var(--color-text-tertiary)] md:block"
                  />
                </>
              )}
              {productGroup && !pathname.startsWith('/tasks/') && (
                <>
                  <span className="hidden text-[var(--color-text-tertiary)] md:inline">
                    {localized(productGroup.title, i18n.language)}
                  </span>
                  <ChevronRight
                    size={14}
                    className="hidden shrink-0 text-[var(--color-text-tertiary)] md:block"
                  />
                </>
              )}
              <span aria-current="page" className="truncate font-medium">
                {routeLabel}
              </span>
            </>
          )}
        </nav>
      </div>

      <div className="flex min-w-0 items-center gap-1 sm:gap-2">
        <Link
          to="/system/messages"
          className="rounded-lg p-2 text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)]"
          aria-label={i18n.language.startsWith('zh') ? '我的收件箱' : 'My inbox'}
        >
          <Bell size={17} />
        </Link>
        {logoutError && (
          <span
            role="alert"
            className="max-w-40 break-words text-xs text-[var(--color-error)] sm:max-w-64"
          >
            {logoutError}
          </span>
        )}
        {user?.tenant_name && (
          <span
            className="hidden max-w-36 items-center gap-1.5 border-r border-[var(--color-border-default)] pr-3 text-xs text-[var(--color-text-secondary)] sm:flex"
            title={user.tenant_name}
          >
            <Building2 size={14} className="shrink-0" />
            <span className="truncate">{user.tenant_name}</span>
          </span>
        )}
        <h2 className="max-w-28 truncate text-sm text-[var(--color-text-primary)] sm:max-w-40">
          <span className="sr-only">{t('auth.welcomeBack')}, </span>
          <span>{user?.display_name || user?.username}</span>
        </h2>
        {user?.roles?.map((role) => (
          <span
            key={role}
            className="hidden rounded-full bg-[var(--color-bg-tertiary)] px-2.5 py-0.5 text-xs text-[var(--color-text-secondary)] xl:inline"
          >
            {role}
          </span>
        ))}
        <button
          onClick={toggleLanguage}
          className="flex items-center gap-1 rounded-lg px-2 py-2 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
          title={t('language.switch')}
          aria-label={t('language.switch')}
        >
          <Languages size={16} />
          <span className="text-xs">{i18n.language === 'zh-CN' ? 'EN' : '中'}</span>
        </button>
        <button
          onClick={toggleTheme}
          className="flex items-center gap-1 rounded-lg px-2 py-2 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
          title={theme === 'dark' ? t('theme.light') : t('theme.dark')}
          aria-label={theme === 'dark' ? t('theme.light') : t('theme.dark')}
        >
          {theme === 'dark' ? <Sun size={16} /> : <Moon size={16} />}
        </button>
        <button
          onClick={handleLogout}
          disabled={loggingOut}
          aria-label={t('auth.logout')}
          className="flex items-center gap-2 rounded-lg px-3 py-2 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
        >
          <LogOut size={16} />
          <span className="hidden sm:inline">{t('auth.logout')}</span>
        </button>
      </div>
    </header>
  );
}
