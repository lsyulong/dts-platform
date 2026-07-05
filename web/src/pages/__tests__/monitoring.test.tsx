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
        expect(screen.getByText('监控告警')).toBeInTheDocument();
      });
    });

    it('renders the subtitle', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('系统运行状态监控与告警配置')).toBeInTheDocument();
      });
    });

    it('renders static system health metrics', () => {
      renderPage();
      expect(screen.getByText('CPU 使用率')).toBeInTheDocument();
      expect(screen.getByText('内存')).toBeInTheDocument();
      expect(screen.getByText('网络 I/O')).toBeInTheDocument();
      expect(screen.getByText('平均延迟')).toBeInTheDocument();
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
        expect(screen.getByText('告警规则')).toBeInTheDocument();
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
        expect(screen.getByText(/延迟 \(ms\) > 500/)).toBeInTheDocument();
        expect(screen.getByText(/错误率 \(%\) > 5/)).toBeInTheDocument();
        expect(screen.getByText(/吞吐量 \(MB\/s\) < 10/)).toBeInTheDocument();
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
        expect(screen.getByText('全局日志')).toBeInTheDocument();
      });
    });

    it('renders log entries in the console', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('系统健康检查通过')).toBeInTheDocument();
        expect(screen.getByText('数据同步进度: 45.2%')).toBeInTheDocument();
        expect(screen.getByText('连接池使用率 > 80%')).toBeInTheDocument();
      });
    });
  });
});
