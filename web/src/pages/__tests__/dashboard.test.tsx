import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { renderWithProviders } from '../../test/test-utils';
import { server } from '../../test/mocks/server';
import { http, HttpResponse } from 'msw';
import DashboardPage from '../dashboard';

describe('DashboardPage', () => {
  beforeAll(() => server.listen());
  afterEach(() => server.resetHandlers());
  afterAll(() => server.close());

  function renderDashboard() {
    return renderWithProviders(<DashboardPage />, { route: '/' });
  }

  describe('loading state', () => {
    it('shows a spinner while loading', () => {
      const { container } = renderDashboard();
      const skeleton = container.querySelector('.animate-pulse');
      expect(skeleton).toBeInTheDocument();
    });
  });

  describe('loaded state', () => {
    it('renders the page header', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.title')).toBeInTheDocument();
      });
    });

    it('renders the subtitle', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.subtitle')).toBeInTheDocument();
      });
    });

    it('displays total tasks count', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.totalTasks')).toBeInTheDocument();
        expect(screen.getByText('12')).toBeInTheDocument();
      });
    });

    it('displays running tasks in subtitle', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.runningCount(3)')).toBeInTheDocument();
      });
    });

    it('displays the share of configured tasks that have not failed', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.successRate')).toBeInTheDocument();
        expect(screen.getByText('95.8%')).toBeInTheDocument();
      });
    });

    it('displays healthy datasources count', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.healthyDatasources')).toBeInTheDocument();
        expect(screen.getByText('7/8')).toBeInTheDocument();
      });
    });

    it('renders recent execution records section', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.recentRuns')).toBeInTheDocument();
      });
    });

    it('renders "dashboard.viewAll" link to tasks page', async () => {
      renderDashboard();
      await waitFor(() => {
        const link = screen.getByText('dashboard.viewAll');
        expect(link).toBeInTheDocument();
        expect(link.getAttribute('href')).toBe('/tasks');
      });
    });

    it('renders recent runs with task names', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('MySQL to PG Sync')).toBeInTheDocument();
        expect(screen.getByText('Mongo to CH')).toBeInTheDocument();
        expect(screen.getByText('Kafka to Redis')).toBeInTheDocument();
      });
    });

    it('renders status badges for recent runs', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('tasks.status.running')).toBeInTheDocument();
        expect(screen.getByText('tasks.status.completed')).toBeInTheDocument();
        expect(screen.getByText('tasks.status.failed')).toBeInTheDocument();
      });
    });

    it('renders chart sections', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.syncVolumeChart')).toBeInTheDocument();
        expect(screen.getByText('dashboard.taskDistribution')).toBeInTheDocument();
      });
    });

    it('renders the log console section', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.systemLog')).toBeInTheDocument();
      });
    });

    it('shows no logs instead of inventing startup or database connection events', async () => {
      renderDashboard();
      expect(await screen.findByText('dashboard.waitingForLogs')).toBeInTheDocument();
      expect(screen.queryByText('dashboard.logStarted')).not.toBeInTheDocument();
      expect(screen.queryByText('dashboard.logDbConnected')).not.toBeInTheDocument();
      expect(screen.getByText('12')).toBeInTheDocument();
    });
  });

  describe('formatBytes utility', () => {
    it('displays formatted bytes for today_synced_bytes', async () => {
      renderDashboard();
      await waitFor(() => {
        // 10737418240 bytes = 10 GB
        expect(screen.getByText('10 GB')).toBeInTheDocument();
      });
    });
  });

  describe('formatNumber utility', () => {
    it('displays formatted number for synced rows', async () => {
      renderDashboard();
      await waitFor(() => {
        // 2500000 = 2.5M - the full subtitle text is "2.5M dashboard.rowsCount"
        expect(screen.getByText(/2\.5M/)).toBeInTheDocument();
      });
    });
  });

  describe('empty state', () => {
    it('keeps chart titles and describes missing data without inventing healthy percentages', async () => {
      server.use(
        http.get('http://localhost:8080/api/v1/monitoring/overview', () =>
          HttpResponse.json({
            total_tasks: 0,
            running_tasks: 0,
            failed_tasks: 0,
            total_datasources: 0,
            healthy_datasources: 0,
            today_synced_rows: 0,
            today_synced_bytes: 0,
            success_rate: 100,
            recent_runs: [],
          }),
        ),
        http.get('http://localhost:8080/api/v1/tasks', () => HttpResponse.json({ data: [] })),
      );
      renderDashboard();
      expect(await screen.findByText('dashboard.syncVolumeChart')).toBeInTheDocument();
      expect(screen.getByText('dashboard.taskDistribution')).toBeInTheDocument();
      expect(screen.getByText('dashboard.noSyncVolume')).toBeInTheDocument();
      expect(screen.getByText('dashboard.noTaskDistribution')).toBeInTheDocument();
      expect(screen.getByText('dashboard.noData')).toBeInTheDocument();
      expect(screen.getByText('datasources.noDatasources')).toBeInTheDocument();
      expect(screen.getAllByText('--')).toHaveLength(2);
      expect(screen.queryByText('100.0%')).not.toBeInTheDocument();
      expect(screen.queryByText('0/0')).not.toBeInTheDocument();
      expect(screen.queryByText('dashboard.connectionHealthy')).not.toBeInTheDocument();
    });

    it('does not label unconfirmed connections as healthy or failed', async () => {
      server.use(
        http.get('http://localhost:8080/api/v1/monitoring/overview', () =>
          HttpResponse.json({
            total_tasks: 2,
            running_tasks: 0,
            failed_tasks: 0,
            total_datasources: 3,
            healthy_datasources: 0,
            today_synced_rows: 0,
            today_synced_bytes: 0,
            success_rate: 100,
            recent_runs: [],
          }),
        ),
      );
      renderDashboard();
      expect(await screen.findByText('0/3')).toBeInTheDocument();
      expect(screen.getByText('dashboard.connectionUnconfirmed')).toBeInTheDocument();
      expect(screen.queryByText('dashboard.connectionHealthy')).not.toBeInTheDocument();
      expect(screen.getByText('100.0%')).toBeInTheDocument();
    });

    it('labels connection health as confirmed only when every connection is healthy', async () => {
      server.use(
        http.get('http://localhost:8080/api/v1/monitoring/overview', () =>
          HttpResponse.json({
            total_tasks: 2,
            running_tasks: 0,
            failed_tasks: 0,
            total_datasources: 3,
            healthy_datasources: 3,
            today_synced_rows: 0,
            today_synced_bytes: 0,
            success_rate: 100,
            recent_runs: [],
          }),
        ),
      );
      renderDashboard();
      expect(await screen.findByText('3/3')).toBeInTheDocument();
      expect(screen.getByText('dashboard.connectionHealthy')).toBeInTheDocument();
      expect(screen.queryByText('dashboard.connectionUnconfirmed')).not.toBeInTheDocument();
    });
  });
});
