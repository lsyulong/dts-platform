import { useNavigate } from 'react-router-dom';
import { LogOut, Sun, Moon, Languages } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '../stores/auth';
import { useAppStore } from '../stores/app';

export default function Header() {
  const { t, i18n } = useTranslation();
  const { user, logout } = useAuthStore();
  const { theme, toggleTheme } = useAppStore();
  const navigate = useNavigate();

  const handleLogout = () => {
    logout();
    navigate('/login');
  };

  const toggleLanguage = () => {
    const next = i18n.language === 'zh-CN' ? 'en-US' : 'zh-CN';
    i18n.changeLanguage(next);
  };

  return (
    <header className="flex h-16 items-center justify-between border-b border-zinc-800/60 bg-zinc-900/40 backdrop-blur-md px-6">
      <div>
        <h2 className="text-sm font-medium text-zinc-400">
          {t('auth.welcomeBack')},{' '}
          <span className="text-zinc-200">{user?.display_name || user?.username}</span>
        </h2>
        <p className="text-xs text-zinc-600">{user?.tenant_name}</p>
      </div>

      <div className="flex items-center gap-2">
        {user?.roles?.map((role) => (
          <span
            key={role}
            className="rounded-full bg-zinc-800 px-2.5 py-0.5 text-xs text-zinc-400"
          >
            {role}
          </span>
        ))}
        <button
          onClick={toggleLanguage}
          className="flex items-center gap-1 rounded-lg px-2 py-2 text-sm text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition-colors"
          title={t('language.switch')}
        >
          <Languages size={16} />
          <span className="text-xs">{i18n.language === 'zh-CN' ? 'EN' : '中'}</span>
        </button>
        <button
          onClick={toggleTheme}
          className="flex items-center gap-1 rounded-lg px-2 py-2 text-sm text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition-colors"
          title={theme === 'dark' ? t('theme.light') : t('theme.dark')}
        >
          {theme === 'dark' ? <Sun size={16} /> : <Moon size={16} />}
        </button>
        <button
          onClick={handleLogout}
          className="flex items-center gap-2 rounded-lg px-3 py-2 text-sm text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition-colors"
        >
          <LogOut size={16} />
          {t('auth.logout')}
        </button>
      </div>
    </header>
  );
}
