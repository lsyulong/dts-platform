import { Outlet, useLocation } from 'react-router-dom';
import Sidebar from '../components/Sidebar';
import Header from '../components/Header';

export default function DashboardLayout() {
  const location = useLocation();
  return (
    <div className="flex h-screen bg-[var(--color-bg-primary)] text-[var(--color-text-primary)]">
      <Sidebar />
      <div className="flex flex-1 flex-col overflow-hidden">
        <Header />
        <main className="flex-1 overflow-y-auto p-6" key={location.pathname}>
          <div className="page-transition">
            <Outlet />
          </div>
        </main>
      </div>
    </div>
  );
}
