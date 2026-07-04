import { NavLink, useLocation } from 'react-router-dom';
import {
  LayoutDashboard, ListTodo, Database, Activity, ChevronLeft, ChevronRight,
} from 'lucide-react';
import { useAppStore } from '../stores/app';

const navItems = [
  { to: '/', icon: LayoutDashboard, label: '仪表盘' },
  { to: '/tasks', icon: ListTodo, label: '任务管理' },
  { to: '/datasources', icon: Database, label: '数据源' },
  { to: '/monitoring', icon: Activity, label: '监控告警' },
];

export default function Sidebar() {
  const { sidebarCollapsed, toggleSidebar } = useAppStore();
  const location = useLocation();

  return (
    <aside
      className={`flex flex-col border-r border-zinc-800/60 bg-zinc-900/40 backdrop-blur-md transition-all duration-300 ${
        sidebarCollapsed ? 'w-16' : 'w-60'
      }`}
    >
      {/* Logo */}
      <div className="flex h-16 items-center justify-between border-b border-zinc-800/60 px-4">
        {!sidebarCollapsed && (
          <span className="text-lg font-bold tracking-tight">
            <span className="text-blue-400">DTS</span> Platform
          </span>
        )}
        <button
          onClick={toggleSidebar}
          className="rounded-lg p-1.5 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-300"
        >
          {sidebarCollapsed ? <ChevronRight size={18} /> : <ChevronLeft size={18} />}
        </button>
      </div>

      {/* Navigation */}
      <nav className="flex-1 space-y-1 p-3">
        {navItems.map(({ to, icon: Icon, label }) => {
          const active = to === '/' ? location.pathname === '/' : location.pathname.startsWith(to);
          return (
            <NavLink
              key={to}
              to={to}
              className={`flex items-center gap-3 rounded-xl px-3 py-2.5 text-sm font-medium transition-all ${
                active
                  ? 'bg-blue-500/10 text-blue-400 shadow-[0_0_20px_rgba(37,99,235,0.1)]'
                  : 'text-zinc-400 hover:bg-zinc-800/50 hover:text-zinc-200'
              }`}
            >
              <Icon size={20} />
              {!sidebarCollapsed && <span>{label}</span>}
            </NavLink>
          );
        })}
      </nav>

      {/* Footer */}
      {!sidebarCollapsed && (
        <div className="border-t border-zinc-800/60 p-4">
          <p className="text-xs text-zinc-600">DTS Platform v0.1.0</p>
        </div>
      )}
    </aside>
  );
}
