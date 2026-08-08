import { describe, it, expect, beforeEach } from 'vitest';
import { useAppStore } from '../app';

describe('useAppStore', () => {
  beforeEach(() => {
    useAppStore.setState({
      sidebarCollapsed: false,
      mobileSidebarOpen: false,
      theme: 'dark',
    });
  });

  describe('initial state', () => {
    it('has sidebarCollapsed false by default', () => {
      const state = useAppStore.getState();
      expect(state.sidebarCollapsed).toBe(false);
    });

    it('has mobileSidebarOpen false by default', () => {
      const state = useAppStore.getState();
      expect(state.mobileSidebarOpen).toBe(false);
    });

    it('has dark theme by default', () => {
      const state = useAppStore.getState();
      expect(state.theme).toBe('dark');
    });
  });

  describe('toggleSidebar', () => {
    it('toggles sidebarCollapsed from false to true', () => {
      useAppStore.getState().toggleSidebar();
      expect(useAppStore.getState().sidebarCollapsed).toBe(true);
    });

    it('toggles sidebarCollapsed from true to false', () => {
      useAppStore.setState({ sidebarCollapsed: true });
      useAppStore.getState().toggleSidebar();
      expect(useAppStore.getState().sidebarCollapsed).toBe(false);
    });

    it('does not affect theme', () => {
      useAppStore.getState().toggleSidebar();
      expect(useAppStore.getState().theme).toBe('dark');
    });
  });

  describe('toggleTheme', () => {
    it('switches theme from dark to light', () => {
      useAppStore.getState().toggleTheme();
      expect(useAppStore.getState().theme).toBe('light');
    });

    it('switches theme from light to dark', () => {
      useAppStore.setState({ theme: 'light' });
      useAppStore.getState().toggleTheme();
      expect(useAppStore.getState().theme).toBe('dark');
    });

    it('does not affect sidebarCollapsed', () => {
      useAppStore.getState().toggleTheme();
      expect(useAppStore.getState().sidebarCollapsed).toBe(false);
    });

    it('can toggle twice to return to original', () => {
      useAppStore.getState().toggleTheme();
      useAppStore.getState().toggleTheme();
      expect(useAppStore.getState().theme).toBe('dark');
    });
  });

  describe('toggleMobileSidebar', () => {
    it('opens mobile sidebar from false to true', () => {
      useAppStore.getState().toggleMobileSidebar();
      expect(useAppStore.getState().mobileSidebarOpen).toBe(true);
    });

    it('closes mobile sidebar from true to false', () => {
      useAppStore.setState({ mobileSidebarOpen: true });
      useAppStore.getState().toggleMobileSidebar();
      expect(useAppStore.getState().mobileSidebarOpen).toBe(false);
    });
  });

  describe('closeMobileSidebar', () => {
    it('sets mobileSidebarOpen to false', () => {
      useAppStore.setState({ mobileSidebarOpen: true });
      useAppStore.getState().closeMobileSidebar();
      expect(useAppStore.getState().mobileSidebarOpen).toBe(false);
    });

    it('is idempotent when already closed', () => {
      useAppStore.getState().closeMobileSidebar();
      expect(useAppStore.getState().mobileSidebarOpen).toBe(false);
    });
  });
});
