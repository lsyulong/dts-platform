import { useNavigate } from 'react-router-dom';
import { LogOut } from 'lucide-react';
import { useAuthStore } from '../stores/auth';

export default function Header() {
  const { user, logout } = useAuthStore();
  const navigate = useNavigate();

  const handleLogout = () => {
    logout();
    navigate('/login');
  };

  return (
    <header className="flex h-16 items-center justify-between border-b border-zinc-800/60 bg-zinc-900/40 backdrop-blur-md px-6">
      <div>
        <h2 className="text-sm font-medium text-zinc-400">
          欢迎回来，<span className="text-zinc-200">{user?.display_name || user?.username}</span>
        </h2>
        <p className="text-xs text-zinc-600">{user?.tenant_name}</p>
      </div>

      <div className="flex items-center gap-4">
        {user?.roles?.map((role) => (
          <span
            key={role}
            className="rounded-full bg-zinc-800 px-2.5 py-0.5 text-xs text-zinc-400"
          >
            {role}
          </span>
        ))}
        <button
          onClick={handleLogout}
          className="flex items-center gap-2 rounded-lg px-3 py-2 text-sm text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition-colors"
        >
          <LogOut size={16} />
          退出
        </button>
      </div>
    </header>
  );
}
