import { describe, it, expect, beforeAll, afterAll, afterEach, beforeEach, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { server } from '../../../test/mocks/server';
import { renderWithProviders, setTestUser } from '../../../test/test-utils';
import TaskListPage from '../index';

describe('TaskListPage', () => {
  beforeEach(() => setTestUser(['task:read', 'task:write', 'task:execute']));
  beforeAll(() => server.listen());
  afterEach(() => server.resetHandlers());
  afterAll(() => server.close());

  function renderPage() {
    return renderWithProviders(<TaskListPage />, { route: '/tasks' });
  }

  it('keeps tasks visible but hides write and execution actions for read-only users', async () => {
    setTestUser(['task:read']);
    renderPage();
    expect(await screen.findByText('MySQL to PostgreSQL Sync')).toBeInTheDocument();
    expect(screen.queryByText('tasks.createTask')).not.toBeInTheDocument();
    expect(screen.queryByTitle('tasks.detail.start')).not.toBeInTheDocument();
    expect(screen.queryByTitle('tasks.detail.cancelRun')).not.toBeInTheDocument();
  });

  describe('loading state', () => {
    it('shows a spinner while loading', () => {
      const { container } = renderPage();
      const skeleton = container.querySelector('.animate-pulse');
      expect(skeleton).toBeInTheDocument();
    });
  });

  describe('loaded state', () => {
    it('renders the page header', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('tasks.title')).toBeInTheDocument();
      });
    });

    it('renders the subtitle', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('tasks.subtitle')).toBeInTheDocument();
      });
    });

    it('renders the create task link button', async () => {
      renderPage();
      await waitFor(() => {
        const createLink = screen.getByText('tasks.createTask');
        expect(createLink).toBeInTheDocument();
        expect(createLink.closest('a')?.getAttribute('href')).toBe('/tasks/create');
      });
    });

    it('renders the search input', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByPlaceholderText(/tasks.searchPlaceholder/)).toBeInTheDocument();
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
        expect(screen.getByText('tasks.status.running')).toBeInTheDocument();
        expect(screen.getByText('tasks.status.draft')).toBeInTheDocument();
        expect(screen.getByText('tasks.status.paused')).toBeInTheDocument();
        expect(screen.getByText('tasks.status.failed')).toBeInTheDocument();
        expect(screen.getByText('tasks.status.completed')).toBeInTheDocument();
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

      const searchInput = screen.getByPlaceholderText(/tasks.searchPlaceholder/);
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

      const searchInput = screen.getByPlaceholderText(/tasks.searchPlaceholder/);
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

      const searchInput = screen.getByPlaceholderText(/tasks.searchPlaceholder/);
      await user.type(searchInput, 'NONEXISTENT');

      await waitFor(() => {
        expect(screen.getByText('tasks.noTasks')).toBeInTheDocument();
        expect(screen.getByText('tasks.createFirst →')).toBeInTheDocument();
      });
    });
  });

  describe('status change actions', () => {
    it('renders play button for draft tasks', async () => {
      renderPage();
      await waitFor(() => screen.getByText('MongoDB to ClickHouse'));
      const playButtons = screen.getAllByTitle('tasks.detail.start');
      expect(playButtons.length).toBeGreaterThan(0);
    });

    it('renders cancel button for running tasks', async () => {
      renderPage();
      await waitFor(() => screen.getByText('MySQL to PostgreSQL Sync'));

      const cancelButtons = screen.getAllByTitle('tasks.detail.cancelRun');
      expect(cancelButtons.length).toBeGreaterThan(0);
    });

    it('renders navigation link to task detail', async () => {
      renderPage();
      await waitFor(() => screen.getByText('MySQL to PostgreSQL Sync'));

      const viewLinks = screen.getAllByText('common.view');
      expect(viewLinks.length).toBe(5);
    });

    it('starts the selected draft task through its real start endpoint', async () => {
      const user = userEvent.setup();
      const started = vi.fn();
      server.use(
        http.post('http://localhost:8080/api/v1/tasks/:id/start', ({ params }) => {
          started(params.id);
          return HttpResponse.json({ id: 'run-new' }, { status: 201 });
        }),
      );
      renderPage();
      const taskName = await screen.findByText('MongoDB to ClickHouse');
      const row = taskName.closest('tr');
      expect(row).not.toBeNull();
      await user.click(within(row!).getByRole('button', { name: 'tasks.detail.start' }));
      await waitFor(() => expect(started).toHaveBeenCalledWith('task-2'));
    });

    it('keeps a running task awaiting confirmation after requesting cancellation', async () => {
      const user = userEvent.setup();
      const cancelled = vi.fn();
      server.use(
        http.patch('http://localhost:8080/api/v1/tasks/:id/status', async ({ request, params }) => {
          cancelled(params.id, await request.json());
          return HttpResponse.json({ status: 'cancelling' }, { status: 202 });
        }),
      );
      renderPage();
      const taskName = await screen.findByText('MySQL to PostgreSQL Sync');
      const row = taskName.closest('tr');
      expect(row).not.toBeNull();
      await user.click(within(row!).getByRole('button', { name: 'tasks.detail.cancelRun' }));
      await waitFor(() =>
        expect(cancelled).toHaveBeenCalledWith('task-1', { status: 'cancelled' }),
      );
      await waitFor(() =>
        expect(
          within(row!).getByRole('button', { name: 'tasks.detail.cancelling' }),
        ).toBeDisabled(),
      );
      expect(within(row!).getByText('tasks.status.running')).toBeInTheDocument();
    });
  });
});
