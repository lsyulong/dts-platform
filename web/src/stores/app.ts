import { create } from 'zustand';

type Theme = 'dark' | 'light';

function getInitialTheme(): Theme {
  try {
    const stored = localStorage.getItem('theme');
    if (stored === 'light' || stored === 'dark') return stored;
  } catch {
    // localStorage may not be available (SSR / test env)
  }
  return 'dark';
}

function applyTheme(theme: Theme) {
  try {
    document.documentElement.classList.toggle('dark', theme === 'dark');
    localStorage.setItem('theme', theme);
  } catch {
    // Ignore if unavailable
  }
}

interface AppState {
  sidebarCollapsed: boolean;
  mobileSidebarOpen: boolean;
  theme: Theme;
  toggleSidebar: () => void;
  toggleMobileSidebar: () => void;
  closeMobileSidebar: () => void;
  toggleTheme: () => void;
}

export const useAppStore = create<AppState>((set) => ({
  sidebarCollapsed: false,
  mobileSidebarOpen: false,
  theme: getInitialTheme(),

  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),

  toggleMobileSidebar: () => set((s) => ({ mobileSidebarOpen: !s.mobileSidebarOpen })),

  closeMobileSidebar: () => set({ mobileSidebarOpen: false }),

  toggleTheme: () =>
    set((s) => {
      const next: Theme = s.theme === 'dark' ? 'light' : 'dark';
      applyTheme(next);
      return { theme: next };
    }),
}));
