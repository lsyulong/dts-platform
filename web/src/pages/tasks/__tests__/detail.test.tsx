import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { Routes, Route } from 'react-router-dom';
import { renderWithProviders } from '../../../test/test-utils';
import TaskDetailPage from '../detail';

// Mock the API client to avoid axios/MSW issues in jsdom
const mockGet = vi.fn();
const mockPatch = vi.fn();
vi.mock('../../../api/client', () => ({
  default: {
    get: (...args: unknown[]) => mockGet(...args),
    patch: (...args: unknown[]) => mockPatch(...args),
  },
}));

describe('TaskDetailPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  const mockTask = {
    id: 'task-1',
    name: 'MySQL to PostgreSQL Sync',
    description: 'Production data sync task',
    source_name: 'prod-mysql',
    source_type: 'MySQL',
    target_name: 'prod-pg',
    target_type: 'PostgreSQL',
    cron_expr: '0 */6 * * *',
    status: 'running',
    config_json: '{}',
    created_at: '2026-01-15T08:00:00Z',
    updated_at: '2026-06-01T12:00:00Z',
    recent_runs: [
      {
        id: 'run-1',
        status: 'running',
        start_time: '2026-07-05T10:00:00Z',
        end_time: null,
        rows_synced: 125000,
        bytes_synced: 524288000,
        total_rows: 500000,
        error_message: null,
      },
      {
        id: 'run-2',
        status: 'completed',
        start_time: '2026-07-04T22:00:00Z',
        end_time: '2026-07-04T23:30:00Z',
        rows_synced: 500000,
        bytes_synced: 2147483648,
        total_rows: 500000,
        error_message: null,
      },
      {
        id: 'run-3',
        status: 'failed',
        start_time: '2026-07-04T16:00:00Z',
        end_time: '2026-07-04T16:05:00Z',
        rows_synced: 0,
        bytes_synced: 0,
        total_rows: 500000,
        error_message: 'Connection timeout: unable to reach source database after 30s',
      },
    ],
  };

  function renderPage(id = 'task-1') {
    return renderWithProviders(
      <Routes>
        <Route path="/tasks/:id" element={<TaskDetailPage />} />
      </Routes>,
      { route: `/tasks/${id}` },
    );
  }

  describe('loading state', () => {
    it('shows a spinner while loading', () => {
      mockGet.mockReturnValueOnce(new Promise(() => {})); // never resolves
      const { container } = renderPage();
      const skeleton = container.querySelector('.animate-pulse');
      expect(skeleton).toBeInTheDocument();
    });
  });

  describe('loaded state', () => {
    beforeEach(() => {
      mockGet.mockResolvedValueOnce({ data: mockTask });
    });

    it('renders the task name', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('MySQL to PostgreSQL Sync')).toBeInTheDocument();
      });
    });

    it('displays truncated task ID', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText(/ID: task-1\.\.\./)).toBeInTheDocument();
      });
    });

    it('displays source-to-target type', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText(/MySQL → PostgreSQL/)).toBeInTheDocument();
      });
    });

    it('displays cron expression when present', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('0 */6 * * *')).toBeInTheDocument();
      });
    });

    it('renders back link to tasks', async () => {
      renderPage();
      await waitFor(() => {
        const links = screen.getAllByRole('link');
        const backLink = links.find(l => l.getAttribute('href') === '/tasks');
        expect(backLink).toBeDefined();
      });
    });

    it('renders source and target metric cards', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('prod-mysql')).toBeInTheDocument();
        expect(screen.getByText('prod-pg')).toBeInTheDocument();
      });
    });

    it('renders the status metric card', async () => {
      renderPage();
      await waitFor(() => {
        // "tasks.status.running" appears both as status value and as a run badge, use getAllByText
        const runningEls = screen.getAllByText('tasks.status.running');
        expect(runningEls.length).toBeGreaterThanOrEqual(1);
      });
    });
  });

  describe('execution history', () => {
    beforeEach(() => {
      mockGet.mockResolvedValueOnce({ data: mockTask });
    });

    it('renders execution history section', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('tasks.detail.executionHistory')).toBeInTheDocument();
      });
    });

    it('displays run entries with statuses', async () => {
      renderPage();
      await waitFor(() => {
        const runningEls = screen.getAllByText('tasks.status.running');
        expect(runningEls.length).toBeGreaterThanOrEqual(2);
        expect(screen.getByText('tasks.status.completed')).toBeInTheDocument();
      });
    });

    it('displays synced rows count for each run', async () => {
      renderPage();
      await waitFor(() => {
        const syncedElements = screen.getAllByText(/tasks.detail.rowsSynced/);
        expect(syncedElements.length).toBeGreaterThan(0);
      });
    });

    it('displays error messages for failed runs', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText(/Connection timeout/)).toBeInTheDocument();
      });
    });
  });

  describe('progress bar', () => {
    it('renders progress bar for running task with active run', async () => {
      mockGet.mockResolvedValueOnce({ data: mockTask });
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('tasks.detail.realTimeProgress')).toBeInTheDocument();
        expect(screen.getByText('25.0%')).toBeInTheDocument();
      });
    });
  });

  describe('status change actions', () => {
    it('renders pause button for running task', async () => {
      mockGet.mockResolvedValueOnce({ data: mockTask });
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('tasks.detail.pause')).toBeInTheDocument();
      });
    });
  });
});
