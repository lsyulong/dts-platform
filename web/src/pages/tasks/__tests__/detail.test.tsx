import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Routes, Route } from 'react-router-dom';
import { renderWithProviders, setTestUser } from '../../../test/test-utils';
import TaskDetailPage from '../detail';
import type { SyncProgress } from '../../../hooks/useTaskProgress';

// Mock the API client to avoid axios/MSW issues in jsdom
const mockGet = vi.fn();
const mockPatch = vi.fn();
const mockPost = vi.fn();
const mockProgress = vi.fn<
  () => {
    connected: boolean;
    accessDenied?: boolean;
    latest: SyncProgress | null;
    history: SyncProgress[];
    completed: boolean;
  }
>();
vi.mock('../../../hooks/useTaskProgress', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useTaskProgress: () => mockProgress(),
}));
vi.mock('../../../api/client', () => ({
  default: {
    get: (...args: unknown[]) => mockGet(...args),
    patch: (...args: unknown[]) => mockPatch(...args),
    post: (...args: unknown[]) => mockPost(...args),
  },
}));

describe('TaskDetailPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    setTestUser(['task:read', 'task:write', 'task:execute']);
    mockProgress.mockReturnValue({ connected: false, latest: null, history: [], completed: false });
    mockGet.mockResolvedValue({
      data: { has_timer: false, cron_expr: null, schedule_history: [] },
    });
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

  it('hides cancellation and schedule changes for read-only users', async () => {
    setTestUser(['task:read']);
    mockGet.mockResolvedValueOnce({ data: mockTask });
    renderPage();
    expect(await screen.findByText('MySQL to PostgreSQL Sync')).toBeInTheDocument();
    expect(screen.queryByText('tasks.detail.cancelRun')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'ops.scheduled' })).not.toBeInTheDocument();
  });

  it('shows a task-log permission refresh notice after a policy close', async () => {
    mockProgress.mockReturnValue({
      connected: false,
      accessDenied: true,
      latest: null,
      history: [],
      completed: false,
    });
    mockGet.mockResolvedValueOnce({ data: mockTask });
    renderPage();
    expect(await screen.findByRole('alert')).toHaveTextContent('tasks.detail.logsAccessDenied');
  });

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
        const backLink = links.find((l) => l.getAttribute('href') === '/tasks');
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
    it('shows unavailable counters instead of claiming zero rows or zero percent', async () => {
      mockProgress.mockReturnValue({
        connected: true,
        latest: {
          task_id: 'task-1',
          run_id: 'run-1',
          status: 'Running',
          rows_synced: 0,
          bytes_synced: 0,
          percent: 0,
          metrics_available: false,
        },
        history: [],
        completed: false,
      });
      mockGet.mockResolvedValueOnce({ data: mockTask });
      renderPage();
      expect(await screen.findByText('tasks.detail.metricsUnavailable')).toBeInTheDocument();
      expect(screen.queryByText('0.0%')).not.toBeInTheDocument();
      expect(screen.queryByText('25.0%')).not.toBeInTheDocument();
    });
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
    it('starts a completed pipeline again through the execution endpoint', async () => {
      const user = userEvent.setup();
      mockGet.mockImplementation((url: string) =>
        Promise.resolve({
          data: url.endsWith('/schedule')
            ? { has_timer: false, schedule_history: [] }
            : { ...mockTask, status: 'completed' },
        }),
      );
      mockPost.mockResolvedValue({ data: { run_id: 'new-run' } });
      renderPage();
      await user.click(await screen.findByRole('button', { name: 'tasks.detail.rerun' }));
      expect(mockPost).toHaveBeenCalledWith('/tasks/task-1/start');
      expect(mockPatch).not.toHaveBeenCalled();
    });
    it('waits for engine confirmation after a cancellation request is accepted', async () => {
      const user = userEvent.setup();
      mockGet.mockImplementation((url: string) =>
        Promise.resolve({
          data: url.endsWith('/schedule') ? { has_timer: false, schedule_history: [] } : mockTask,
        }),
      );
      mockPatch.mockResolvedValue({ data: { status: 'cancelling' } });
      renderPage();
      await user.click(await screen.findByText('tasks.detail.cancelRun'));
      expect(await screen.findByRole('button', { name: 'tasks.detail.cancelling' })).toBeDisabled();
      expect(screen.getAllByText('tasks.status.running').length).toBeGreaterThan(0);
      expect(mockPatch).toHaveBeenCalledWith('/tasks/task-1/status', { status: 'cancelled' });
    });
    it('renders cancel button for running task', async () => {
      mockGet.mockResolvedValueOnce({ data: mockTask });
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('tasks.detail.cancelRun')).toBeInTheDocument();
      });
    });

    it('keeps the task visible and shows an error when status changes fail', async () => {
      const user = userEvent.setup();
      mockGet.mockImplementation((url: string) =>
        Promise.resolve({
          data: url.endsWith('/schedule') ? { has_timer: false, schedule_history: [] } : mockTask,
        }),
      );
      mockPatch.mockRejectedValue(new Error('engine unavailable'));
      renderPage();
      await user.click(await screen.findByText('tasks.detail.cancelRun'));
      expect(
        await screen.findByText('tasks.updateStatusError: engine unavailable'),
      ).toBeInTheDocument();
      expect(screen.getByText('MySQL to PostgreSQL Sync')).toBeInTheDocument();
    });
  });

  it('shows a retryable error instead of redirecting when task loading fails', async () => {
    mockGet.mockRejectedValue(new Error('network unavailable'));
    renderPage();
    expect(await screen.findByText('tasks.detail.loadError')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'common.retry' })).toBeInTheDocument();
  });

  it('shows no execution records even when the scheduler is unavailable', async () => {
    mockGet.mockImplementation((url: string) =>
      url.endsWith('/schedule')
        ? Promise.reject(new Error('scheduler unavailable'))
        : Promise.resolve({ data: { ...mockTask, recent_runs: [] } }),
    );
    renderPage();
    expect(await screen.findByText('tasks.detail.noExecutionRecords')).toBeInTheDocument();
    expect(await screen.findByText('tasks.detail.scheduleError')).toBeInTheDocument();
  });

  it('preserves the full engine error for diagnosis', async () => {
    const message = 'Connection timeout: ' + 'source database unreachable; '.repeat(8);
    mockGet.mockResolvedValueOnce({
      data: { ...mockTask, recent_runs: [{ ...mockTask.recent_runs[2], error_message: message }] },
    });
    renderPage();
    expect(await screen.findByText(message.trim())).toBeInTheDocument();
  });

  it('disables scheduling when the API explicitly reports an unconfigured scheduler', async () => {
    mockGet.mockImplementation((url: string) =>
      Promise.resolve({
        data: url.endsWith('/schedule')
          ? { enabled: false, has_timer: false, schedule_history: [] }
          : mockTask,
      }),
    );
    renderPage();
    expect(await screen.findByText('tasks.detail.schedulerUnavailable')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'ops.scheduled' })).not.toBeInTheDocument();
  });

  it('labels scheduler command dispatch records without treating them as pipeline execution', async () => {
    mockGet.mockImplementation((url: string) =>
      Promise.resolve({
        data: url.endsWith('/schedule')
          ? {
              enabled: true,
              has_timer: true,
              active: true,
              state: 'active',
              cron_expr: '0 */6 * * *',
              schedule_history: [
                {
                  id: 1,
                  name: 'dts-trigger-1',
                  status: 'triggered',
                  created_time: '2026-10-08T08:00:00Z',
                },
              ],
            }
          : { ...mockTask, recent_runs: [] },
      }),
    );
    renderPage();
    expect((await screen.findAllByText('ops.dispatched')).length).toBeGreaterThan(0);
    expect(screen.getByRole('heading', { name: 'ops.dispatchHistory' })).toBeInTheDocument();
    expect(screen.getByText('ops.dispatchHistoryHint')).toBeInTheDocument();
    expect(screen.getAllByText('2026-10-08T08:00:00Z').length).toBeGreaterThan(0);
    expect(screen.getByText('dts-trigger-1')).toBeInTheDocument();
    expect(screen.getByText('tasks.detail.noExecutionRecords')).toBeInTheDocument();
    expect(screen.queryByText(/exit:/)).not.toBeInTheDocument();
    expect(screen.queryByText('tasks.status.completed')).not.toBeInTheDocument();
  });

  it('does not label a saved timer template as actively scheduled', async () => {
    mockGet.mockImplementation((url: string) =>
      Promise.resolve({
        data: url.endsWith('/schedule')
          ? {
              enabled: true,
              has_timer: true,
              active: false,
              state: 'pending',
              cron_expr: '0 */6 * * *',
              schedule_history: [],
            }
          : mockTask,
      }),
    );
    renderPage();
    expect(await screen.findByText('ops.scheduleStates.pending')).toBeInTheDocument();
    expect(screen.getByText('Cron (UTC):')).toBeInTheDocument();
    expect(screen.queryByText('ops.scheduleStates.active')).not.toBeInTheDocument();
  });

  it('keeps the cron input visible when scheduling fails', async () => {
    const user = userEvent.setup();
    mockGet.mockImplementation((url: string) =>
      Promise.resolve({
        data: url.endsWith('/schedule')
          ? { enabled: true, has_timer: false, schedule_history: [] }
          : mockTask,
      }),
    );
    mockPost.mockRejectedValue({ response: { data: { message: 'Scheduler unavailable' } } });
    renderPage();
    await user.click(await screen.findByRole('button', { name: 'ops.scheduled' }));
    await user.type(screen.getByPlaceholderText('0 */6 * * *'), '0 */6 * * *');
    await user.click(screen.getByRole('button', { name: 'common.confirm' }));
    expect(
      await screen.findByText('tasks.detail.scheduleUpdateError: Scheduler unavailable'),
    ).toBeInTheDocument();
    expect(screen.getByPlaceholderText('0 */6 * * *')).toHaveValue('0 */6 * * *');
  });
});
