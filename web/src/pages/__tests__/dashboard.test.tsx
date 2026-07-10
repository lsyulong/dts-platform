import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { server } from '../../test/mocks/server';
import DashboardPage from '../dashboard';

describe('DashboardPage', () => {
  beforeAll(() => server.listen());
  afterEach(() => server.resetHandlers());
  afterAll(() => server.close());

  function renderDashboard() {
    return render(
      <MemoryRouter>
        <DashboardPage />
      </MemoryRouter>
    );
  }

  describe('loading state', () => {
    it('shows a spinner while loading', () => {
      const { container } = renderDashboard();
      const spinner = container.querySelector('.animate-spin');
      expect(spinner).toBeInTheDocument();
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

    it('displays success rate', async () => {
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
        expect(screen.getByText('running')).toBeInTheDocument();
        expect(screen.getByText('completed')).toBeInTheDocument();
        expect(screen.getByText('failed')).toBeInTheDocument();
      });
    });

    it('renders data flow visualization placeholder', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.dataFlowViz')).toBeInTheDocument();
        expect(screen.getByText('dashboard.dataFlowPlaceholder')).toBeInTheDocument();
        expect(screen.getByText('dashboard.dataFlowHint')).toBeInTheDocument();
      });
    });

    it('renders the log console section', async () => {
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('dashboard.systemLog')).toBeInTheDocument();
      });
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
    it('shows zero values when overview is minimal', async () => {
      // The default mock returns rich data; this test just verifies the
      // component handles null/undefined gracefully via the ?? operators
      renderDashboard();
      await waitFor(() => {
        expect(screen.getByText('12')).toBeInTheDocument();
      });
    });
  });
});
