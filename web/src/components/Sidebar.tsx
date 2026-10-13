import { useEffect, useState, useCallback } from 'react';
import { NavLink, useLocation } from 'react-router-dom';
import {
  LayoutDashboard,
  ListTodo,
  Activity,
  ChevronLeft,
  ChevronRight,
  Settings,
  Shield,
  Plug,
  Search,
  Sparkles,
  ChevronDown,
  Database,
  FolderTree,
  Layers,
  BookOpen,
  TableProperties,
  Sigma,
  Boxes,
  LockKeyhole,
  ShieldCheck,
  Fingerprint,
  ScanLine,
  ClipboardCheck,
  Network,
  BarChart3,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '../stores/app';
import { useAuthStore } from '../stores/auth';
import { productGroups, productModules } from '../product/catalog';
import { localized } from '../product/ui-text';
import type { NavigationNode, NavigationPolicyState } from '../product/navigation';

interface NavItem {
  to: string;
  icon: React.ComponentType<{ size?: number }>;
  label: string;
  depth?: number;
}

interface NavGroup {
  id: string;
  section?: string;
  items: NavItem[];
}

export default function Sidebar({ navigation }: { navigation?: NavigationPolicyState } = {}) {
  const { t, i18n } = useTranslation();
  const {
    sidebarCollapsed,
    mobileSidebarOpen,
    toggleSidebar,
    toggleMobileSidebar,
    closeMobileSidebar,
  } = useAppStore();
  const location = useLocation();
  const user = useAuthStore((state) => state.user);

  const [isMobile, setIsMobile] = useState(() => window.innerWidth < 1024);
  const [closedGroups, setClosedGroups] = useState<Record<string, boolean>>({});
  const [navigationSearch, setNavigationSearch] = useState('');

  useEffect(() => {
    setClosedGroups({});
  }, [location.pathname]);

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

  const groupIcons: Record<string, NavItem['icon']> = {
    workbench: LayoutDashboard,
    integration: Plug,
    warehouse: Layers,
    standards: BookOpen,
    models: TableProperties,
    metrics: Sigma,
    development: Settings,
    metadata: Network,
    'master-data': Boxes,
    'data-security': LockKeyhole,
    'sensitive-data': ScanLine,
    privacy: ShieldCheck,
    watermark: Fingerprint,
    quality: ClipboardCheck,
    assets: FolderTree,
    services: Network,
    bi: BarChart3,
    support: Activity,
    access: Shield,
  };
  const legacyLabels: Record<string, string> = {
    '/': 'nav.dashboard',
    '/datasources': 'nav.datasources',
    '/tasks': 'nav.tasks',
    '/query': 'nav.query',
    '/analysis': 'nav.analysis',
    '/monitoring': 'nav.monitoring',
    '/ops': 'nav.ops',
  };
  let navGroups: NavGroup[] = productGroups
    .map((group) => ({
      id: group.id,
      section:
        group.id === 'integration'
          ? t('nav.sections.integration')
          : localized(group.title, i18n.language),
      items: productModules
        .filter((module) => module.groupId === group.id)
        .filter(
          (module) => !navigation || navigation.nodes.some((node) => node.path === module.path),
        )
        .filter(
          (module) =>
            !['users', 'roles', 'permissions', 'tenants'].includes(module.id) ||
            user?.permissions.includes('user:manage'),
        )
        .map((module) => ({
          to: module.path,
          icon:
            module.path === '/tasks'
              ? ListTodo
              : module.path === '/query'
                ? Search
                : module.path === '/analysis'
                  ? Sparkles
                  : (groupIcons[group.id] ?? Database),
          label: legacyLabels[module.path]
            ? t(legacyLabels[module.path])
            : localized(module.title, i18n.language),
        }))
        .filter(
          (item) =>
            !navigationSearch ||
            item.label.toLocaleLowerCase().includes(navigationSearch.toLocaleLowerCase()),
        ),
    }))
    .filter((group) => group.items.length > 0);
  if (navigation?.policy?.mode === 'assigned' || navigation?.policy?.mode === 'expired') {
    const items: NavItem[] = [];
    const visit = (nodes: NavigationNode[], depth: number) => {
      for (const node of nodes.sort(
        (a, b) => a.sort_order - b.sort_order || a.id.localeCompare(b.id),
      )) {
        const module = productModules.find((module) => module.path === node.path);
        const label =
          node.source === 'template'
            ? node.name
            : module
              ? localized(module.title, i18n.language)
              : node.name;
        if (
          !navigationSearch ||
          label.toLocaleLowerCase().includes(navigationSearch.toLocaleLowerCase())
        )
          items.push({
            to: node.path,
            icon: groupIcons[module?.groupId ?? 'access'] ?? Database,
            label,
            depth,
          });
        visit(
          navigation.nodes.filter((child) => child.parent_id === node.id),
          depth + 1,
        );
      }
    };
    visit(
      navigation.nodes.filter((node) => node.parent_id === null),
      0,
    );
    navGroups = items.length
      ? [
          {
            id: 'tenant-navigation',
            section: i18n.language.startsWith('zh') ? '当前租户菜单' : 'Tenant navigation',
            items,
          },
        ]
      : [];
  }
  if (!navigation && user?.permissions.includes('user:manage') && !navigationSearch) {
    navGroups
      .find((group) => group.id === 'access')
      ?.items.unshift({ to: '/system', icon: Shield, label: t('nav.system') });
  }
  if (!navigation && !navigationSearch) {
    navGroups
      .find((group) => group.id === 'development')
      ?.items.push(
        { to: '/monitoring', icon: Activity, label: t('nav.monitoring') },
        { to: '/ops', icon: Settings, label: t('nav.ops') },
      );
    navGroups
      .find((group) => group.id === 'bi')
      ?.items.push({ to: '/analysis', icon: Sparkles, label: t('nav.analysis') });
  }

  const renderNavItem = ({ to, icon: Icon, label, depth = 0 }: NavItem) => {
    const active = to === '/' ? location.pathname === '/' : location.pathname.startsWith(to);
    return (
      <NavLink
        key={to}
        to={to}
        end={to === '/'}
        aria-label={label}
        title={sidebarCollapsed && !isMobile ? label : undefined}
        onClick={handleNavClick}
        style={depth > 0 ? { paddingLeft: `${12 + depth * 14}px` } : undefined}
        className={`flex items-center gap-3 rounded-md border-l-2 px-3 py-2.5 text-sm font-medium transition-colors ${
          active
            ? 'border-[var(--color-nav-active)] bg-[var(--color-nav-hover)] text-[var(--color-nav-active)]'
            : 'border-transparent text-[var(--color-nav-text)] hover:bg-[var(--color-nav-hover)] hover:text-white'
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
      className={`flex h-full shrink-0 flex-col bg-[var(--color-nav-bg)] transition-all duration-300 ${
        isMobile ? 'w-60' : sidebarCollapsed ? 'w-16' : 'w-60'
      }`}
    >
      {/* Logo */}
      <div className="flex h-16 shrink-0 items-center justify-between border-b border-white/10 px-4">
        {!sidebarCollapsed || isMobile ? (
          <div>
            <span className="flex items-center gap-2 text-base font-semibold tracking-tight text-white">
              <Database size={22} className="text-[var(--color-nav-active)]" />
              <span className="text-[var(--color-nav-active)]">DTS</span>{' '}
              {t('common.platformLabel')}
            </span>
            <p className="mt-1 max-w-44 truncate text-[10px] leading-tight text-[var(--color-nav-muted)]">
              {t('common.appTagline')}
            </p>
          </div>
        ) : null}
        <button
          onClick={handleToggle}
          className="rounded-md p-1.5 text-[var(--color-nav-text)] hover:bg-[var(--color-nav-hover)] hover:text-white"
          aria-label={t('nav.toggleMenu')}
        >
          {sidebarCollapsed && !isMobile ? <ChevronRight size={18} /> : <ChevronLeft size={18} />}
        </button>
      </div>

      {(!sidebarCollapsed || isMobile) && (
        <div className="px-3 pt-3">
          <label className="relative block">
            <Search size={14} className="absolute left-2.5 top-2.5 text-[var(--color-nav-muted)]" />
            <input
              aria-label={i18n.language.startsWith('zh') ? '搜索功能模块' : 'Search modules'}
              placeholder={i18n.language.startsWith('zh') ? '搜索功能模块' : 'Search modules'}
              value={navigationSearch}
              onChange={(event) => setNavigationSearch(event.target.value)}
              className="w-full rounded-md border border-white/10 bg-white/5 py-2 pl-8 pr-2 text-xs text-white placeholder-[var(--color-nav-muted)]"
            />
          </label>
        </div>
      )}
      <nav className="flex-1 space-y-3 overflow-y-auto px-2 py-4">
        {navGroups.map((group) => (
          <div key={group.id}>
            {group.section && (!sidebarCollapsed || isMobile) && (
              <button
                type="button"
                aria-expanded={!!navigationSearch || !closedGroups[group.id]}
                aria-controls={`nav-group-${group.id}`}
                onClick={() =>
                  setClosedGroups((prev) => ({ ...prev, [group.id]: !prev[group.id] }))
                }
                className="mb-1 flex w-full items-center justify-between px-3 py-2 text-xs font-medium text-[var(--color-nav-muted)] hover:text-white"
              >
                {group.section}
                <ChevronDown size={13} className={closedGroups[group.id] ? '-rotate-90' : ''} />
              </button>
            )}
            {(navigationSearch || !closedGroups[group.id] || (sidebarCollapsed && !isMobile)) && (
              <div id={`nav-group-${group.id}`} className="space-y-1">
                {group.items.map(renderNavItem)}
              </div>
            )}
          </div>
        ))}
      </nav>

      {/* Footer */}
      {(!sidebarCollapsed || isMobile) && (
        <div className="border-t border-white/10 p-4">
          <p className="text-xs text-[var(--color-nav-muted)]">{t('common.version')}</p>
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
      {isMobile && mobileSidebarOpen && (
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
