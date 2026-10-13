import { act, renderHook } from '@testing-library/react';
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { useTaskProgress } from '../useTaskProgress';
import { useAuthStore } from '../../stores/auth';
import { clearSession } from '../../lib/session';

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  onopen: (() => void) | null = null;
  onclose: ((event: { code?: number }) => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  constructor(
    public url: URL,
    public protocols: string[],
  ) {
    MockWebSocket.instances.push(this);
  }
  close = vi.fn(() => this.onclose?.({ code: 1000 }));
}

describe('useTaskProgress', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
    localStorage.clear();
    useAuthStore.setState({ token: null });
    vi.useFakeTimers();
    vi.stubGlobal('WebSocket', MockWebSocket);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('does not connect without a token', () => {
    renderHook(() => useTaskProgress('task-1'));
    expect(MockWebSocket.instances).toHaveLength(0);
  });

  it('authenticates using a subprotocol and stops reconnecting after unmount', () => {
    localStorage.setItem('token', 'test-token');
    useAuthStore.setState({ token: 'test-token' });
    const { unmount } = renderHook(() => useTaskProgress('task-1'));
    const ws = MockWebSocket.instances[0];
    expect(ws.protocols).toEqual(['dts.logs', 'bearer.test-token']);
    expect(ws.url.pathname).toBe('/ws/tasks/task-1');
    expect(ws.url.search).toBe('');
    unmount();
    act(() => {
      vi.advanceTimersByTime(10000);
    });
    expect(MockWebSocket.instances).toHaveLength(1);
  });

  it('resets old progress when the task changes and does not reconnect completed runs', () => {
    localStorage.setItem('token', 'test-token');
    useAuthStore.setState({ token: 'test-token' });
    const { result, rerender } = renderHook(({ id }) => useTaskProgress(id), {
      initialProps: { id: 'task-1' },
    });
    act(() => {
      const ws = MockWebSocket.instances[0];
      ws.onmessage?.({
        data: JSON.stringify({
          run_id: 'run-1',
          status: 'Completed',
          rows_synced: 5,
          percent: 100,
        }),
      });
      ws.close();
      vi.advanceTimersByTime(10000);
    });
    expect(MockWebSocket.instances).toHaveLength(1);
    expect(result.current.completed).toBe(true);
    rerender({ id: 'task-2' });
    expect(result.current.latest).toBeNull();
    expect(result.current.completed).toBe(false);
    expect(MockWebSocket.instances).toHaveLength(2);
  });

  it('replaces the connection when the token changes and closes it on session clearing', () => {
    localStorage.setItem('token', 'opaque-first');
    useAuthStore.setState({ token: 'opaque-first' });
    renderHook(() => useTaskProgress('task-1'));
    const first = MockWebSocket.instances[0];
    act(() => {
      localStorage.setItem('token', 'opaque-second');
      useAuthStore.setState({ token: 'opaque-second' });
    });
    expect(first.close).toHaveBeenCalledTimes(1);
    expect(MockWebSocket.instances[1].protocols).toEqual(['dts.logs', 'bearer.opaque-second']);
    act(() => {
      clearSession();
      vi.advanceTimersByTime(10000);
    });
    expect(MockWebSocket.instances[1].close).toHaveBeenCalledTimes(1);
    expect(MockWebSocket.instances).toHaveLength(2);
  });

  it('stops reconnecting after a policy close while retaining the login session', () => {
    localStorage.setItem('token', 'opaque-session');
    useAuthStore.setState({ token: 'opaque-session' });
    const { result } = renderHook(() => useTaskProgress('task-1'));
    act(() => {
      MockWebSocket.instances[0].onclose?.({ code: 1008 });
      vi.advanceTimersByTime(10000);
    });
    expect(result.current.accessDenied).toBe(true);
    expect(result.current.connected).toBe(false);
    expect(MockWebSocket.instances).toHaveLength(1);
    expect(useAuthStore.getState().token).toBe('opaque-session');
  });

  it('retries a network close instead of reporting lost access', () => {
    useAuthStore.setState({ token: 'opaque-session' });
    const { result } = renderHook(() => useTaskProgress('task-1'));
    act(() => {
      MockWebSocket.instances[0].onclose?.({ code: 1006 });
      vi.advanceTimersByTime(5000);
    });
    expect(MockWebSocket.instances).toHaveLength(2);
    expect(result.current.accessDenied).toBe(false);
    expect(MockWebSocket.instances[1].protocols).toEqual(['dts.logs', 'bearer.opaque-session']);
  });
});
