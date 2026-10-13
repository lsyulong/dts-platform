import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import { renderWithProviders } from '../../test/test-utils';
import OpsCenterPage from '../ops';

const mockScheduleStatus = vi.fn();
vi.mock('../../hooks/useApi', () => ({
  useTasks: () => ({ data: [{ id: 'task-1', name: 'Orders', status: 'draft' }], isLoading: false }),
  useScheduleStatus: () => mockScheduleStatus(),
}));

describe('OpsCenterPage scheduling state', () => {
  it.each([
    { active: true, state: 'active' },
    { active: false, state: 'pending' },
    { active: false, state: 'offline' },
  ])('shows the actual $state state independently of a saved template', ({ active, state }) => {
    mockScheduleStatus.mockReturnValue({
      isLoading: false,
      data: { has_timer: true, active, state, cron_expr: '0 */6 * * *', schedule_history: [] },
    });
    renderWithProviders(<OpsCenterPage />);
    expect(screen.getByText(`ops.scheduleStates.${state}`)).toBeInTheDocument();
    expect(screen.getByText(/0 \*\/6 \* \* \* \(UTC\)/)).toBeInTheDocument();
    if (!active) expect(screen.queryByText('ops.scheduleStates.active')).not.toBeInTheDocument();
  });

  it('does not present a failed schedule lookup as an unscheduled task', () => {
    mockScheduleStatus.mockReturnValue({
      isLoading: false,
      error: { response: { data: { message: 'Scheduler unavailable' } } },
    });
    renderWithProviders(<OpsCenterPage />);
    expect(screen.getByRole('alert')).toHaveTextContent('Scheduler unavailable');
    expect(screen.queryByText('ops.notScheduled')).not.toBeInTheDocument();
  });
});
