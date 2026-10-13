import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../../test/test-utils';
import { server } from '../../test/mocks/server';
import MonitoringPage from '../monitoring';

describe('MonitoringPage', () => {
  beforeAll(() => server.listen());
  afterEach(() => server.resetHandlers());
  afterAll(() => server.close());

  function renderPage() {
    return renderWithProviders(<MonitoringPage />, { route: '/monitoring' });
  }

  describe('loading state', () => {
    it('shows a spinner in the alert rules section while loading', () => {
      const { container } = renderPage();
      const spinner = container.querySelector('.animate-spin');
      expect(spinner).toBeInTheDocument();
    });
  });

  describe('loaded state', () => {
    it('renders the page header', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('monitoring.title')).toBeInTheDocument();
      });
    });

    it('renders the subtitle', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('monitoring.subtitle')).toBeInTheDocument();
      });
    });

    it('renders the uncollected metric categories', () => {
      renderPage();
      expect(screen.getByText('monitoring.cpuUsage')).toBeInTheDocument();
      expect(screen.getByText('monitoring.memory')).toBeInTheDocument();
      expect(screen.getByText('monitoring.networkIO')).toBeInTheDocument();
      expect(screen.getByText('monitoring.avgLatency')).toBeInTheDocument();
    });

    it('marks unsupported host metrics as uncollected instead of displaying fixed values', () => {
      renderPage();
      expect(screen.getAllByText('monitoring.notCollected')).toHaveLength(4);
      for (const value of ['42', '18.4', '845', '45', 'Normal', 'Total 32 GB', 'P99: 120ms']) {
        expect(screen.queryByText(value)).not.toBeInTheDocument();
      }
    });

    it('renders alert rules section header', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('monitoring.alertRules')).toBeInTheDocument();
      });
    });
  });

  describe('alert rules list', () => {
    it('renders alert rules after loading', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('高延迟告警')).toBeInTheDocument();
        expect(screen.getByText('错误率过高')).toBeInTheDocument();
        expect(screen.getByText('吞吐量过低')).toBeInTheDocument();
      });
    });

    it('displays metric labels for alerts', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText(/metrics.latency > 500/)).toBeInTheDocument();
        expect(screen.getByText(/metrics.error_rate > 5/)).toBeInTheDocument();
        expect(screen.getByText(/metrics.throughput < 10/)).toBeInTheDocument();
      });
    });

    it('renders toggle switches for each alert', async () => {
      const { container } = renderPage();
      await waitFor(() => {
        const toggleButtons = container.querySelectorAll('.relative.h-6.w-11.rounded-full');
        expect(toggleButtons.length).toBe(3);
      });
    });

    it('toggles alert enabled state optimistically', async () => {
      const user = userEvent.setup();
      const { container } = renderPage();

      await waitFor(() => screen.getByText('高延迟告警'));

      const toggleButtons = container.querySelectorAll('.relative.h-6.w-11.rounded-full');
      // First alert (高延迟告警) is enabled, its toggle has bg-[var(--color-action)]
      expect(toggleButtons[0]?.className).toContain('bg-[var(--color-action)]');

      // Click the third toggle (吞吐量过低, which is disabled)
      if (toggleButtons[2]) {
        await user.click(toggleButtons[2]);
        // After click, it should toggle optimistically to enabled (bg-[var(--color-action)])
        await waitFor(() => {
          const refreshedButtons = container.querySelectorAll('.relative.h-6.w-11.rounded-full');
          expect(refreshedButtons[2]?.className).toContain('bg-[var(--color-action)]');
        });
      }
    });
  });

  describe('log console', () => {
    it('renders the global log console', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('dashboard.globalLog')).toBeInTheDocument();
      });
    });

    it('shows no logs instead of displaying example health and sync events', async () => {
      renderPage();
      expect(screen.getByText('dashboard.waitingForLogs')).toBeInTheDocument();
      expect(screen.queryByText('System health check passed')).not.toBeInTheDocument();
      expect(screen.queryByText('Data sync progress: 45.2%')).not.toBeInTheDocument();
      expect(screen.queryByText('Connection pool usage > 80%')).not.toBeInTheDocument();
      expect(await screen.findByText('高延迟告警')).toBeInTheDocument();
    });
  });
});
