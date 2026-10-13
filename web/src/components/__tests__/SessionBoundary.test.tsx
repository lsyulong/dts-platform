import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import SessionBoundary from '../SessionBoundary';
import { useAuthStore } from '../../stores/auth';

const mockGet = vi.fn();
const mockPost = vi.fn();
vi.mock('../../api/client', () => ({
  default: {
    get: (...args: unknown[]) => mockGet(...args),
    post: (...args: unknown[]) => mockPost(...args),
  },
}));

const user = {
  id: 'user-1',
  username: 'reader',
  email: 'reader@example.invalid',
  tenant_id: 'tenant-1',
  tenant_name: 'workspace',
  roles: [],
  permissions: ['task:read'],
};

function renderBoundary() {
  return render(
    <MemoryRouter initialEntries={['/']}>
      <Routes>
        <Route path="/login" element={<div>Login page</div>} />
        <Route
          path="/"
          element={
            <SessionBoundary>
              <div>Protected workspace</div>
            </SessionBoundary>
          }
        />
      </Routes>
    </MemoryRouter>,
  );
}

describe('SessionBoundary', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    localStorage.clear();
    localStorage.setItem('token', 'opaque-restored-session');
    useAuthStore.setState({
      token: 'opaque-restored-session',
      user: null,
      initialized: false,
      restoring: false,
      restoreError: null,
      loading: false,
    });
  });

  it('verifies an opaque restored token before rendering protected routes', async () => {
    let finish: ((value: { data: typeof user }) => void) | undefined;
    mockGet.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    renderBoundary();
    expect(mockGet).toHaveBeenCalledWith('/auth/me');
    expect(screen.queryByText('Protected workspace')).not.toBeInTheDocument();
    await act(async () => {
      finish?.({ data: user });
    });
    expect(await screen.findByText('Protected workspace')).toBeInTheDocument();
    expect(useAuthStore.getState().user).toEqual(user);
    expect(localStorage.getItem('token')).toBe('opaque-restored-session');
  });

  it('retains a session after network failure and allows explicit verification retry', async () => {
    mockGet
      .mockRejectedValueOnce(new Error('Network unavailable'))
      .mockResolvedValueOnce({ data: user });
    const interactions = userEvent.setup();
    renderBoundary();
    expect(await screen.findByText('auth.restoreFailed: Network unavailable')).toBeInTheDocument();
    expect(localStorage.getItem('token')).toBe('opaque-restored-session');
    expect(screen.queryByText('Protected workspace')).not.toBeInTheDocument();
    await interactions.click(screen.getByRole('button', { name: 'common.retry' }));
    expect(await screen.findByText('Protected workspace')).toBeInTheDocument();
    expect(mockGet).toHaveBeenCalledTimes(2);
    expect(mockPost).not.toHaveBeenCalled();
  });

  it('returns expired sessions to login without recursively attempting server logout', async () => {
    mockGet.mockRejectedValue({ response: { status: 401 } });
    renderBoundary();
    expect(await screen.findByText('Login page')).toBeInTheDocument();
    expect(useAuthStore.getState().token).toBeNull();
    expect(localStorage.getItem('token')).toBeNull();
    expect(mockPost).not.toHaveBeenCalled();
  });
});
