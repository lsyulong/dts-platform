import axios from 'axios';
import { clearSession } from '../lib/session';

const API_BASE = import.meta.env.VITE_API_BASE_URL || 'http://localhost:8080/api/v1';

const client = axios.create({
  baseURL: API_BASE,
  timeout: 30000,
  headers: { 'Content-Type': 'application/json' },
});

// Tokens are opaque; only the server interprets them.
client.interceptors.request.use((config) => {
  const token = localStorage.getItem('token');
  if (token && !config.headers.Authorization) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

// Response interceptor: handle 401
client.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401 && !error.config?.url?.endsWith('/auth/login')) {
      const authorization = error.config?.headers?.Authorization;
      clearSession(
        typeof authorization === 'string' ? authorization.replace(/^Bearer /, '') : null,
      );
    }
    return Promise.reject(error);
  },
);

export default client;
