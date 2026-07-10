import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './i18n';
import './index.css';

// Initialize theme from localStorage/dark mode
const savedTheme = localStorage.getItem('theme') || 'dark';
document.documentElement.classList.toggle('dark', savedTheme === 'dark');
// Prevent flash of transitions on initial load
document.documentElement.classList.add('no-transitions');
window.addEventListener('load', () => {
  document.documentElement.classList.remove('no-transitions');
});

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
