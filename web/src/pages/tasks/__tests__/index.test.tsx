import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { server } from '../../../test/mocks/server';
import TaskListPage from '../index';

describe('TaskListPage', () => {
  beforeAll(() => server.listen());
  afterEach(() => server.resetHandlers());
  afterAll(() => server.close());

  function renderPage() {
    return render(
      <MemoryRouter initialEntries={['/tasks']}>
        <TaskListPage />
      </MemoryRouter>
    );
  }

  describe('loading state', () => {
    it('shows a spinner while loading', () => {
      const { container } = renderPage();
      const spinner = container.querySelector('.animate-spin');
      expect(spinner).toBeInTheDocument();
    });
  });

  describe('loaded state', () => {
    it('renders the page header', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('任务管理')).toBeInTheDocument();
      });
    });

    it('renders the subtitle', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('管理所有数据同步任务')).toBeInTheDocument();
      });
    });

    it('renders the create task link button', async () => {
      renderPage();
      await waitFor(() => {
        const createLink = screen.getByText('创建任务');
        expect(createLink).toBeInTheDocument();
        expect(createLink.closest('a')?.getAttribute('href')).toBe('/tasks/create');
      });
    });

    it('renders the search input', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByPlaceholderText(/搜索任务名称或描述/)).toBeInTheDocument();
      });
    });

    it('displays all tasks from the API', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('MySQL to PostgreSQL Sync')).toBeInTheDocument();
        expect(screen.getByText('MongoDB to ClickHouse')).toBeInTheDocument();
        expect(screen.getByText('Kafka to Redis Cache')).toBeInTheDocument();
        expect(screen.getByText('Oracle Migration')).toBeInTheDocument();
        expect(screen.getByText('Completed Backup Sync')).toBeInTheDocument();
      });
    });

    it('displays task status labels in Chinese', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('运行中')).toBeInTheDocument();
        expect(screen.getByText('草稿')).toBeInTheDocument();
        expect(screen.getByText('已暂停')).toBeInTheDocument();
        expect(screen.getByText('失败')).toBeInTheDocument();
        expect(screen.getByText('已完成')).toBeInTheDocument();
      });
    });

    it('displays cron expressions when present', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('cron: 0 */6 * * *')).toBeInTheDocument();
      });
    });

    it('renders task links to detail pages', async () => {
      renderPage();
      await waitFor(() => {
        const taskLink = screen.getByText('MySQL to PostgreSQL Sync');
        expect(taskLink.closest('a')?.getAttribute('href')).toBe('/tasks/task-1');
      });
    });
  });

  describe('search filtering', () => {
    it('filters tasks by name on search input', async () => {
      const user = userEvent.setup();
      renderPage();

      await waitFor(() => screen.getByText('MySQL to PostgreSQL Sync'));

      const searchInput = screen.getByPlaceholderText(/搜索任务名称或描述/);
      await user.type(searchInput, 'MySQL');

      await waitFor(() => {
        expect(screen.getByText('MySQL to PostgreSQL Sync')).toBeInTheDocument();
        expect(screen.queryByText('Kafka to Redis Cache')).not.toBeInTheDocument();
      });
    });

    it('filters tasks by description', async () => {
      const user = userEvent.setup();
      renderPage();

      await waitFor(() => screen.getByText('MySQL to PostgreSQL Sync'));

      const searchInput = screen.getByPlaceholderText(/搜索任务名称或描述/);
      await user.clear(searchInput);
      await user.type(searchInput, 'Real-time');

      await waitFor(() => {
        expect(screen.getByText('Kafka to Redis Cache')).toBeInTheDocument();
        expect(screen.queryByText('MySQL to PostgreSQL Sync')).not.toBeInTheDocument();
      });
    });

    it('shows empty state when search matches no tasks', async () => {
      const user = userEvent.setup();
      renderPage();

      await waitFor(() => screen.getByText('MySQL to PostgreSQL Sync'));

      const searchInput = screen.getByPlaceholderText(/搜索任务名称或描述/);
      await user.type(searchInput, 'NONEXISTENT');

      await waitFor(() => {
        expect(screen.getByText('暂无同步任务')).toBeInTheDocument();
        expect(screen.getByText('创建第一个任务 →')).toBeInTheDocument();
      });
    });
  });

  describe('status change actions', () => {
    it('renders play button for draft tasks', async () => {
      renderPage();
      await waitFor(() => screen.getByText('MongoDB to ClickHouse'));
      // The play buttons are in a hidden (opacity-0) group, visible on hover
      const playButtons = screen.getAllByTitle('启动');
      expect(playButtons.length).toBeGreaterThan(0);
    });

    it('renders pause button for running tasks', async () => {
      renderPage();
      await waitFor(() => screen.getByText('MySQL to PostgreSQL Sync'));

      const pauseButtons = screen.getAllByTitle('暂停');
      expect(pauseButtons.length).toBeGreaterThan(0);
    });

    it('renders navigation link to task detail', async () => {
      renderPage();
      await waitFor(() => screen.getByText('MySQL to PostgreSQL Sync'));

      const viewLinks = screen.getAllByText('查看');
      expect(viewLinks.length).toBe(5);
    });
  });
});
