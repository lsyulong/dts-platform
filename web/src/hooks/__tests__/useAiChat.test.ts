import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useAiChat } from '../useAiChat';
import { useAuthStore } from '../../stores/auth';

describe('useAiChat', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('clears an expired session from a native fetch without calling logout', async () => {
    localStorage.setItem('token', 'opaque');
    useAuthStore.setState({ token: 'opaque' });
    const fetchMock = vi.fn().mockResolvedValue(new Response('', { status: 401 }));
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAiChat());
    await act(async () => {
      await result.current.sendMessage('Hi');
    });
    expect(useAuthStore.getState().token).toBeNull();
    expect(localStorage.getItem('token')).toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('does not clear a newer login when a previous AI request expires', async () => {
    localStorage.setItem('token', 'first-session');
    useAuthStore.setState({ token: 'first-session' });
    let finish: ((value: Response) => void) | undefined;
    vi.stubGlobal(
      'fetch',
      vi.fn().mockReturnValue(
        new Promise<Response>((resolve) => {
          finish = resolve;
        }),
      ),
    );
    const { result } = renderHook(() => useAiChat());
    let pending: Promise<void> | undefined;
    act(() => {
      pending = result.current.sendMessage('Hi');
    });
    localStorage.setItem('token', 'new-session');
    useAuthStore.setState({ token: 'new-session' });
    await act(async () => {
      finish?.(new Response('', { status: 401 }));
      await pending;
    });
    expect(useAuthStore.getState().token).toBe('new-session');
    expect(localStorage.getItem('token')).toBe('new-session');
  });

  it('reports a disabled service without fabricating an assistant response', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        new Response(JSON.stringify({ message: 'AI 服务未启用' }), { status: 503 }),
      );
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAiChat());
    await act(async () => {
      await result.current.sendMessage('Explain the sync failure');
    });
    expect(result.current.isUnavailable).toBe(true);
    expect(result.current.messages).toEqual([
      { role: 'user', content: 'Explain the sync failure' },
    ]);
    await act(async () => {
      await result.current.sendMessage('Retry');
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('uses the configured API address and preserves a fragmented response', async () => {
    const encoder = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode('data: hel'));
        controller.enqueue(encoder.encode('lo\n\ndata: [DONE]\n\n'));
        controller.close();
      },
    });
    const fetchMock = vi.fn().mockResolvedValue(new Response(body));
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAiChat());
    await act(async () => {
      await result.current.sendMessage('Hi');
    });
    expect(result.current.messages[1]).toEqual({ role: 'assistant', content: 'hello' });
    expect(fetchMock.mock.calls[0][0]).toContain('http://localhost:8080/api/v1/ai/chat?message=Hi');
  });
});
