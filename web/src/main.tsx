import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClientProvider } from '@tanstack/react-query';
import { queryClient } from './lib/queryClient';
import App from './App';
import './i18n';
import './index.css';
import { useAppStore } from './stores/app';

const savedTheme = useAppStore.getState().theme;
document.documentElement.classList.toggle('dark', savedTheme === 'dark');
// Prevent flash of transitions on initial load
document.documentElement.classList.add('no-transitions');
window.addEventListener('load', () => {
  document.documentElement.classList.remove('no-transitions');
});

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
