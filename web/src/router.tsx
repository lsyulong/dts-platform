import { createBrowserRouter } from 'react-router-dom';
import SessionBoundary from './components/SessionBoundary';
import DashboardLayout from './layouts/DashboardLayout';
import { productModules } from './product/catalog';
import ProductRoute from './product/ProductRoute';
const legacyRoutes = new Set([
  '/',
  '/tasks',
  '/datasources',
  '/monitoring',
  '/ops',
  '/system',
  '/query',
  '/analysis',
]);

export const router = createBrowserRouter([
  {
    path: '/login',
    lazy: async () => ({ Component: (await import('./pages/login')).default }),
  },
  {
    path: '/',
    element: (
      <SessionBoundary>
        <DashboardLayout />
      </SessionBoundary>
    ),
    children: [
      { index: true, lazy: async () => ({ Component: (await import('./pages/dashboard')).default }) },
      { path: 'tasks', lazy: async () => ({ Component: (await import('./pages/tasks/index')).default }) },
      { path: 'tasks/create', lazy: async () => ({ Component: (await import('./pages/tasks/create')).default }) },
      { path: 'tasks/:id', lazy: async () => ({ Component: (await import('./pages/tasks/detail')).default }) },
      { path: 'datasources', lazy: async () => ({ Component: (await import('./pages/datasources')).default }) },
      { path: 'monitoring', lazy: async () => ({ Component: (await import('./pages/monitoring')).default }) },
      { path: 'ops', lazy: async () => ({ Component: (await import('./pages/ops')).default }) },
      { path: 'system', lazy: async () => ({ Component: (await import('./pages/system')).default }) },
      { path: 'query', lazy: async () => ({ Component: (await import('./pages/query')).default }) },
      { path: 'analysis', lazy: async () => ({ Component: (await import('./pages/analysis')).default }) },
      ...productModules
        .filter((module) => !legacyRoutes.has(module.path))
        .map((module) => ({
          path: module.path.slice(1),
          element: <ProductRoute module={module} />,
        })),
    ],
  },
]);
