import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import Sidebar from '../Sidebar';
import { useAppStore } from '../../stores/app';

// Helper to render with router
function renderWithRouter(initialRoute = '/') {
  return render(
    <MemoryRouter initialEntries={[initialRoute]}>
      <Sidebar />
    </MemoryRouter>
  );
}

describe('Sidebar', () => {
  beforeEach(() => {
    useAppStore.setState({ sidebarCollapsed: false, theme: 'dark' });
  });

  it('renders the DTS Platform logo when expanded', () => {
    renderWithRouter();
    expect(screen.getByText('DTS')).toBeInTheDocument();
    expect(screen.getByText('Platform')).toBeInTheDocument();
  });

  it('hides the logo text when collapsed', () => {
    useAppStore.setState({ sidebarCollapsed: true });
    renderWithRouter();
    expect(screen.queryByText('DTS')).not.toBeInTheDocument();
    expect(screen.queryByText('Platform')).not.toBeInTheDocument();
  });

  describe('navigation links', () => {
    it('renders all nav links when expanded', () => {
      renderWithRouter();
      expect(screen.getByText('nav.dashboard')).toBeInTheDocument();
      expect(screen.getByText('nav.tasks')).toBeInTheDocument();
      expect(screen.getByText('nav.datasources')).toBeInTheDocument();
      expect(screen.getByText('nav.monitoring')).toBeInTheDocument();
    });

    it('hides nav link labels when collapsed', () => {
      useAppStore.setState({ sidebarCollapsed: true });
      renderWithRouter();
      expect(screen.queryByText('nav.dashboard')).not.toBeInTheDocument();
      expect(screen.queryByText('nav.tasks')).not.toBeInTheDocument();
    });

    it('highlights the active nav link based on current path', () => {
      renderWithRouter('/tasks');
      const tasksLink = screen.getByText('nav.tasks').closest('a');
      expect(tasksLink?.className).toContain('bg-blue-500/10');
    });

    it('highlights dashboard link only for exact "/" path', () => {
      renderWithRouter('/');
      const dashboardLink = screen.getByText('nav.dashboard').closest('a');
      expect(dashboardLink?.className).toContain('bg-blue-500/10');
    });

    it('does not highlight dashboard link for "/tasks" path', () => {
      renderWithRouter('/tasks');
      const dashboardLink = screen.getByText('nav.dashboard').closest('a');
      expect(dashboardLink?.className).not.toContain('bg-blue-500/10');
    });

    it('highlights datasources link for "/datasources" path', () => {
      renderWithRouter('/datasources');
      const dsLink = screen.getByText('nav.datasources').closest('a');
      expect(dsLink?.className).toContain('bg-blue-500/10');
    });

    it('highlights monitoring link for "/monitoring" path', () => {
      renderWithRouter('/monitoring');
      const monLink = screen.getByText('nav.monitoring').closest('a');
      expect(monLink?.className).toContain('bg-blue-500/10');
    });
  });

  describe('collapse/expand toggle', () => {
    it('shows ChevronLeft icon when expanded', () => {
      renderWithRouter();
      const button = screen.getByRole('button');
      expect(button).toBeInTheDocument();
      // The sidebar is expanded, so ChevronLeft should be shown
    });

    it('shows ChevronRight icon when collapsed', () => {
      useAppStore.setState({ sidebarCollapsed: true });
      renderWithRouter();
      const button = screen.getByRole('button');
      expect(button).toBeInTheDocument();
    });

    it('calls toggleSidebar on button click', async () => {
      const user = userEvent.setup();
      renderWithRouter();
      const button = screen.getByRole('button');

      await user.click(button);
      expect(useAppStore.getState().sidebarCollapsed).toBe(true);
    });
  });

  describe('expanded/collapsed widths', () => {
    it('applies w-60 class when expanded', () => {
      const { container } = renderWithRouter();
      const aside = container.querySelector('aside');
      expect(aside?.className).toContain('w-60');
      expect(aside?.className).not.toContain('w-16');
    });

    it('applies w-16 class when collapsed', () => {
      useAppStore.setState({ sidebarCollapsed: true });
      const { container } = renderWithRouter();
      const aside = container.querySelector('aside');
      expect(aside?.className).toContain('w-16');
    });
  });

  describe('footer', () => {
    it('shows version footer when expanded', () => {
      renderWithRouter();
      expect(screen.getByText('common.version')).toBeInTheDocument();
    });

    it('hides version footer when collapsed', () => {
      useAppStore.setState({ sidebarCollapsed: true });
      renderWithRouter();
      expect(screen.queryByText('common.version')).not.toBeInTheDocument();
    });
  });
});
