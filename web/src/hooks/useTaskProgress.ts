import { useEffect, useState, useRef } from 'react';
import { useAuthStore } from '../stores/auth';

export interface SyncProgress {
  run_id: string;
  task_id: string;
  status: 'Running' | 'Completed' | 'Failed' | 'Cancelled';
  rows_synced: number;
  bytes_synced: number;
  total_rows?: number;
  current_table?: string;
  percent: number;
  metrics_available?: boolean;
  message?: string;
}

interface ProgressState {
  connected: boolean;
  accessDenied: boolean;
  latest: SyncProgress | null;
  history: SyncProgress[];
  completed: boolean;
}

export function useTaskProgress(taskId: string | undefined) {
  const token = useAuthStore((state) => state.token);
  const [state, setState] = useState<ProgressState>({
    connected: false,
    accessDenied: false,
    latest: null,
    history: [],
    completed: false,
  });
  const wsRef = useRef<WebSocket | null>(null);
  useEffect(() => {
    let disposed = false;
    let completed = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
    const initialState: ProgressState = {
      connected: false,
      accessDenied: false,
      latest: null,
      history: [],
      completed: false,
    };
    setState(initialState);

    const connect = () => {
      if (!taskId || !token || disposed) return;
      const apiUrl = new URL(
        import.meta.env.VITE_API_BASE_URL || 'http://localhost:8080/api/v1',
        window.location.href,
      );
      apiUrl.protocol = apiUrl.protocol === 'https:' ? 'wss:' : 'ws:';
      apiUrl.pathname = `/ws/tasks/${encodeURIComponent(taskId)}`;
      apiUrl.search = '';
      apiUrl.hash = '';
      const ws = new WebSocket(apiUrl, ['dts.logs', `bearer.${token}`]);
      wsRef.current = ws;

      ws.onopen = () => {
        if (!disposed) setState((s) => ({ ...s, connected: true }));
      };

      ws.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data) as { type?: string } & SyncProgress;

          let progress: SyncProgress | null = null;

          if (msg.run_id && msg.status) {
            progress = msg as SyncProgress;
          } else if (typeof msg === 'object' && 'rows_synced' in msg) {
            progress = msg as unknown as SyncProgress;
          }

          if (progress && !disposed) {
            completed = ['Completed', 'Failed', 'Cancelled'].includes(progress.status);
            setState((s) => ({
              ...s,
              latest: progress,
              history: [...s.history, progress].slice(-100),
              completed,
            }));
          }
        } catch {
          // Log text and heartbeat frames do not carry progress data.
        }
      };

      ws.onclose = (event) => {
        if (disposed) return;
        const accessDenied = event.code === 1008;
        setState((s) => ({ ...s, connected: false, accessDenied }));
        if (accessDenied) clearTimeout(reconnectTimer);
        else if (!completed) reconnectTimer = setTimeout(connect, 5000);
      };

      ws.onerror = () => {
        ws.close();
      };
    };
    connect();
    return () => {
      disposed = true;
      clearTimeout(reconnectTimer);
      wsRef.current?.close();
      wsRef.current = null;
    };
  }, [taskId, token]);

  return state;
}

/** 格式化字节数 */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

/** 格式化行数 */
export function formatRows(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}
