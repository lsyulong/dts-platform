import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { server } from '../../test/mocks/server';
import MonitoringPage from '../monitoring';

describe('MonitoringPage', () => {
  beforeAll(() => server.listen());
  afterEach(() => server.resetHandlers());
  afterAll(() => server.close());

  function renderPage() {
    return render(
      <MemoryRouter>
        <MonitoringPage />
      </MemoryRouter>
    );
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

    it('renders static system health metrics', () => {
      renderPage();
      expect(screen.getByText('monitoring.cpuUsage')).toBeInTheDocument();
      expect(screen.getByText('monitoring.memory')).toBeInTheDocument();
      expect(screen.getByText('monitoring.networkIO')).toBeInTheDocument();
      expect(screen.getByText('monitoring.avgLatency')).toBeInTheDocument();
    });

    it('displays CPU value of 42%', () => {
      renderPage();
      expect(screen.getByText('42')).toBeInTheDocument();
    });

    it('displays memory value of 18.4 GB', () => {
      renderPage();
      expect(screen.getByText('18.4')).toBeInTheDocument();
      expect(screen.getByText('GB')).toBeInTheDocument();
    });

    it('displays network I/O value', () => {
      renderPage();
      expect(screen.getByText('845')).toBeInTheDocument();
      expect(screen.getByText('MB/s')).toBeInTheDocument();
    });

    it('displays average latency value', () => {
      renderPage();
      expect(screen.getByText('45')).toBeInTheDocument();
      expect(screen.getByText('ms')).toBeInTheDocument();
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
      // First alert (高延迟告警) is enabled, its toggle has bg-blue-500
      expect(toggleButtons[0]?.className).toContain('bg-blue-500');

      // Click the third toggle (吞吐量过低, which is disabled)
      if (toggleButtons[2]) {
        await user.click(toggleButtons[2]);
        // After click, it should toggle optimistically to enabled (bg-blue-500)
        await waitFor(() => {
          const refreshedButtons = container.querySelectorAll('.relative.h-6.w-11.rounded-full');
          expect(refreshedButtons[2]?.className).toContain('bg-blue-500');
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

    it('renders log entries in the console', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('System health check passed')).toBeInTheDocument();
        expect(screen.getByText('Data sync progress: 45.2%')).toBeInTheDocument();
        expect(screen.getByText('Connection pool usage > 80%')).toBeInTheDocument();
      });
    });
  });
});
