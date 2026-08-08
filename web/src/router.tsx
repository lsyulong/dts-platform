import { createBrowserRouter, Navigate } from 'react-router-dom';
import DashboardLayout from './layouts/DashboardLayout';
import LoginPage from './pages/login';
import DashboardPage from './pages/dashboard';
import TaskListPage from './pages/tasks/index';
import TaskCreatePage from './pages/tasks/create';
import TaskDetailPage from './pages/tasks/detail';
import DatasourcesPage from './pages/datasources';
import MonitoringPage from './pages/monitoring';
import OpsCenterPage from './pages/ops';
import SystemSettingsPage from './pages/system';
import QueryPage from './pages/query';
import AnalysisPage from './pages/analysis';

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const token = localStorage.getItem('token');
  if (!token) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

export const router = createBrowserRouter([
  {
    path: '/login',
    element: <LoginPage />,
  },
  {
    path: '/',
    element: (
      <ProtectedRoute>
        <DashboardLayout />
      </ProtectedRoute>
    ),
    children: [
      { index: true, element: <DashboardPage /> },
      { path: 'tasks', element: <TaskListPage /> },
      { path: 'tasks/create', element: <TaskCreatePage /> },
      { path: 'tasks/:id', element: <TaskDetailPage /> },
      { path: 'datasources', element: <DatasourcesPage /> },
      { path: 'monitoring', element: <MonitoringPage /> },
      { path: 'ops', element: <OpsCenterPage /> },
      { path: 'system', element: <SystemSettingsPage /> },
      { path: 'query', element: <QueryPage /> },
      { path: 'analysis', element: <AnalysisPage /> },
    ],
  },
]);
