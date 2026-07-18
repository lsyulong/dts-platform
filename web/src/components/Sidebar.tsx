import { useEffect, useState, useCallback } from 'react';
import { NavLink, useLocation } from 'react-router-dom';
import {
  LayoutDashboard, ListTodo, Database, Activity, ChevronLeft, ChevronRight,
  Settings, Shield, Plug, Search, Sparkles,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '../stores/app';

interface NavItem {
  to: string;
  icon: React.ComponentType<{ size?: number }>;
  label: string;
}

interface NavGroup {
  section?: string;
  items: NavItem[];
}

export default function Sidebar() {
  const { t } = useTranslation();
  const {
    sidebarCollapsed,
    mobileSidebarOpen,
    toggleSidebar,
    toggleMobileSidebar,
    closeMobileSidebar,
  } = useAppStore();
  const location = useLocation();

  const [isMobile, setIsMobile] = useState(() => window.innerWidth < 1024);

  useEffect(() => {
    let timeoutId: ReturnType<typeof setTimeout>;
    const handleResize = () => {
      clearTimeout(timeoutId);
      timeoutId = setTimeout(() => {
        setIsMobile(window.innerWidth < 1024);
      }, 100);
    };
    window.addEventListener('resize', handleResize);
    return () => {
      window.removeEventListener('resize', handleResize);
      clearTimeout(timeoutId);
    };
  }, []);

  // Close mobile sidebar when switching to desktop
  useEffect(() => {
    if (!isMobile) {
      closeMobileSidebar();
    }
  }, [isMobile, closeMobileSidebar]);

  const handleNavClick = useCallback(() => {
    if (isMobile) {
      closeMobileSidebar();
    }
  }, [isMobile, closeMobileSidebar]);

  const handleToggle = () => {
    if (isMobile) {
      toggleMobileSidebar();
    } else {
      toggleSidebar();
    }
  };

  const navGroups: NavGroup[] = [
    {
      items: [
        { to: '/', icon: LayoutDashboard, label: t('nav.dashboard') },
      ],
    },
    {
      section: t('nav.sections.integration'),
      items: [
        { to: '/datasources', icon: Plug, label: t('nav.datasources') },
        { to: '/tasks', icon: ListTodo, label: t('nav.tasks') },
      ],
    },
    {
      section: t('nav.sections.development'),
      items: [
        { to: '/ops', icon: Settings, label: t('nav.ops') },
      ],
    },
    {
      section: t('nav.sections.consumption'),
      items: [
        { to: '/query', icon: Search, label: t('nav.query') },
        { to: '/analysis', icon: Sparkles, label: t('nav.analysis') },
      ],
    },
    {
      section: t('nav.sections.governance'),
      items: [
        { to: '/monitoring', icon: Activity, label: t('nav.monitoring') },
        { to: '/system', icon: Shield, label: t('nav.system') },
      ],
    },
  ];

  const renderNavItem = ({ to, icon: Icon, label }: NavItem) => {
    const active = to === '/' ? location.pathname === '/' : location.pathname.startsWith(to);
    return (
      <NavLink
        key={to}
        to={to}
        onClick={handleNavClick}
        className={`flex items-center gap-3 rounded-xl px-3 py-2.5 text-sm font-medium transition-all ${
          active
            ? 'bg-blue-500/10 text-blue-400 shadow-[0_0_20px_rgba(37,99,235,0.1)]'
            : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)]'
        }`}
      >
        <Icon size={20} />
        {(!sidebarCollapsed || isMobile) && <span>{label}</span>}
      </NavLink>
    );
  };

  const sidebarContent = (
    <aside
      aria-label={t('nav.toggleMenu')}
      className={`flex h-full flex-col border-r border-[var(--color-border-default)] bg-[var(--color-bg-elevated)] backdrop-blur-md transition-all duration-300 ${
        isMobile
          ? 'w-60'
          : sidebarCollapsed
            ? 'w-16'
            : 'w-60'
      }`}
    >
      {/* Logo */}
      <div className="flex h-16 items-center justify-between border-b border-[var(--color-border-default)] px-4">
        {!sidebarCollapsed || isMobile ? (
          <div>
            <span className="text-lg font-bold tracking-tight">
              <span className="text-blue-400">DTS</span> 数据中台
            </span>
            <p className="text-[10px] leading-tight text-[var(--color-text-disabled)]">
              {t('common.appTagline')}
            </p>
          </div>
        ) : null}
        <button
          onClick={handleToggle}
          className="rounded-lg p-1.5 text-[var(--color-text-tertiary)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-secondary)]"
          aria-label={sidebarCollapsed ? t('nav.toggleMenu') : undefined}
        >
          {(sidebarCollapsed && !isMobile) ? <ChevronRight size={18} /> : <ChevronLeft size={18} />}
        </button>
      </div>

      {/* Navigation */}
      <nav className="flex-1 space-y-4 p-3">
        {navGroups.map((group, gi) => (
          <div key={gi}>
            {group.section && (!sidebarCollapsed || isMobile) && (
              <p className="mb-1 px-3 text-[10px] font-semibold uppercase tracking-wider text-[var(--color-text-disabled)]">
                {group.section}
              </p>
            )}
            <div className="space-y-1">
              {group.items.map(renderNavItem)}
            </div>
          </div>
        ))}
      </nav>

      {/* Footer */}
      {(!sidebarCollapsed || isMobile) && (
        <div className="border-t border-[var(--color-border-default)] p-4">
          <p className="text-xs text-[var(--color-text-disabled)]">{t('common.version')}</p>
        </div>
      )}
    </aside>
  );

  return (
    <>
      {/* Mobile overlay */}
      {isMobile && mobileSidebarOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/50 backdrop-blur-sm lg:hidden"
          onClick={closeMobileSidebar}
          aria-hidden="true"
        />
      )}

      {/* Desktop: normal flex child */}
      {!isMobile && sidebarContent}

      {/* Mobile: fixed drawer */}
      {isMobile && (
        <div
          className={`fixed inset-y-0 left-0 z-50 transition-transform duration-300 ${
            mobileSidebarOpen ? 'translate-x-0' : '-translate-x-full'
          }`}
        >
          {sidebarContent}
        </div>
      )}
    </>
  );
}
