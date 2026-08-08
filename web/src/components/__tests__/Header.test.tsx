import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import Header from '../Header';
import { useAuthStore } from '../../stores/auth';
import { useAppStore } from '../../stores/app';

const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...(actual as Record<string, unknown>),
    useNavigate: () => mockNavigate,
  };
});

describe('Header', () => {
  beforeEach(() => {
    useAuthStore.setState({
      token: 'test-token',
      user: {
        id: 'u1',
        username: 'admin',
        email: 'admin@dts.io',
        display_name: '管理员',
        tenant_id: 't1',
        tenant_name: 'default',
        roles: ['admin', 'operator'],
        permissions: ['task:read', 'task:write'],
      },
      loading: false,
    });
    useAppStore.setState({
      mobileSidebarOpen: false,
      sidebarCollapsed: false,
      theme: 'dark',
    });
    mockNavigate.mockClear();
  });

  function renderHeader() {
    return render(
      <MemoryRouter>
        <Header />
      </MemoryRouter>
    );
  }

  it('displays the welcome message with display name', () => {
    renderHeader();
    expect(screen.getByText(/auth.welcomeBack/)).toBeInTheDocument();
    expect(screen.getByText('管理员')).toBeInTheDocument();
  });

  it('falls back to username when display_name is not set', () => {
    useAuthStore.setState({
      user: {
        id: 'u2',
        username: 'operator1',
        email: 'op@dts.io',
        tenant_id: 't1',
        tenant_name: 'default',
        roles: [],
        permissions: [],
      },
    });
    renderHeader();
    expect(screen.getByText('operator1')).toBeInTheDocument();
  });

  it('displays the tenant name', () => {
    renderHeader();
    expect(screen.getByText('default')).toBeInTheDocument();
  });

  it('renders role badges for each role', () => {
    renderHeader();
    expect(screen.getByText('admin')).toBeInTheDocument();
    expect(screen.getByText('operator')).toBeInTheDocument();
  });

  it('renders logout button', () => {
    renderHeader();
    expect(screen.getByText('auth.logout')).toBeInTheDocument();
  });

  it('renders hamburger menu button with correct aria-label', () => {
    renderHeader();
    const menuButton = screen.getByLabelText('nav.toggleMenu');
    expect(menuButton).toBeInTheDocument();
    expect(menuButton.className).toContain('lg:hidden');
  });

  it('toggles mobile sidebar on hamburger click', async () => {
    const user = userEvent.setup();
    renderHeader();
    const menuButton = screen.getByLabelText('nav.toggleMenu');
    await user.click(menuButton);
    expect(useAppStore.getState().mobileSidebarOpen).toBe(true);
  });

  describe('logout behavior', () => {
    it('calls logout and navigates to /login on logout click', async () => {
      const user = userEvent.setup();
      renderHeader();

      await user.click(screen.getByText('auth.logout'));

      expect(useAuthStore.getState().token).toBeNull();
      expect(useAuthStore.getState().user).toBeNull();
      expect(mockNavigate).toHaveBeenCalledWith('/login');
    });
  });

  describe('user states', () => {
    it('renders gracefully when user is null', () => {
      useAuthStore.setState({ user: null });
      renderHeader();
      expect(screen.getByText('auth.logout')).toBeInTheDocument();
    });

    it('renders no role badges when user has no roles', () => {
      useAuthStore.setState({
        user: {
          id: 'u3',
          username: 'guest',
          email: 'g@dts.io',
          tenant_id: 't1',
          tenant_name: 'default',
          roles: [],
          permissions: [],
        },
      });
      const { container } = renderHeader();
      // No role spans should be rendered when roles array is empty
      const roleBadges = container.querySelectorAll('[class*="rounded-full"]');
      expect(roleBadges.length).toBe(0);
    });
  });
});
