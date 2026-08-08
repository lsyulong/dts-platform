import { useEffect, useState, useRef, useCallback } from 'react';

export interface SyncProgress {
  run_id: string;
  task_id: string;
  status: 'Running' | 'Completed' | 'Failed' | 'Cancelled';
  rows_synced: number;
  bytes_synced: number;
  total_rows?: number;
  current_table?: string;
  percent: number;
  message?: string;
}

interface ProgressState {
  connected: boolean;
  latest: SyncProgress | null;
  history: SyncProgress[];
  completed: boolean;
}

const WS_BASE = (typeof window !== 'undefined' && window.location.hostname === 'localhost')
  ? 'ws://localhost:8080'
  : `ws://${window.location.hostname}:8080`;

export function useTaskProgress(taskId: string | undefined) {
  const [state, setState] = useState<ProgressState>({
    connected: false,
    latest: null,
    history: [],
    completed: false,
  });
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const connect = useCallback(() => {
    if (!taskId) return;

    const ws = new WebSocket(`${WS_BASE}/ws/tasks/${taskId}`);
    wsRef.current = ws;

    ws.onopen = () => {
      setState((s) => ({ ...s, connected: true }));
    };

    ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data) as { type?: string } & SyncProgress;

        // 处理两种消息格式：直接 SyncProgress 或 WsHub 包裹格式
        // WsHub 推送格式：(task_id, json_payload) 通过 broadcast
        // 实际可能收到直接 JSON 或数组

        let progress: SyncProgress | null = null;

        // 直接 SyncProgress
        if (msg.run_id && msg.status) {
          progress = msg as SyncProgress;
        }
        // 其他包裹格式 fallback
        else if (typeof msg === 'object' && 'rows_synced' in msg) {
          progress = msg as unknown as SyncProgress;
        }

        if (progress) {
          setState((s) => ({
            ...s,
            latest: progress,
            history: [...s.history, progress].slice(-100), // 最多保留 100 条
            completed: ['Completed', 'Failed', 'Cancelled'].includes(progress.status),
          }));
        }
      } catch {
        // 忽略解析失败的 JSON
      }
    };

    ws.onclose = () => {
      setState((s) => ({ ...s, connected: false }));
      // 5 秒后自动重连（除非已完成）
      setState((prev) => {
        if (!prev.completed) {
          reconnectTimer.current = setTimeout(connect, 5000);
        }
        return prev;
      });
    };

    ws.onerror = () => {
      ws.close();
    };
  }, [taskId]);

  useEffect(() => {
    connect();
    return () => {
      wsRef.current?.close();
      clearTimeout(reconnectTimer.current);
    };
  }, [connect]);

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
