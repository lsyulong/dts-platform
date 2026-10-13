import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AiReportPanel } from '../AiReportPanel';
import { useAuthStore } from '../../../stores/auth';

describe('AiReportPanel', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it('shows AI disabled as an unavailable state and prevents repeated requests', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        new Response(JSON.stringify({ message: 'AI 服务未启用' }), { status: 503 }),
      );
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();
    render(<AiReportPanel taskId="task-1" />);
    await user.click(screen.getByRole('button', { name: 'ai.generateReport' }));
    expect(await screen.findByRole('status')).toHaveTextContent('ai.disabled');
    expect(screen.getByRole('button', { name: 'ai.generateReport' })).toBeDisabled();
    expect(screen.queryByText('ai.reportFailed')).not.toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('expires the shared session when report fetch returns unauthorized', async () => {
    localStorage.setItem('token', 'opaque-report-session');
    useAuthStore.setState({ token: 'opaque-report-session' });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status: 401 })));
    const user = userEvent.setup();
    render(<AiReportPanel taskId="task-1" />);
    await user.click(screen.getByRole('button', { name: 'ai.generateReport' }));
    expect(useAuthStore.getState().token).toBeNull();
    expect(localStorage.getItem('token')).toBeNull();
  });

  it('preserves split stream chunks and renders HTML as text', async () => {
    const encoder = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode('da'));
        controller.enqueue(
          encoder.encode('ta: <img src=x onerror=alert(1)>\\n第一行\\n第二行\n\n'),
        );
        controller.enqueue(encoder.encode('data: [DONE]\n\n'));
        controller.close();
      },
    });
    const fetchMock = vi.fn().mockResolvedValue(new Response(body));
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();
    const { container } = render(<AiReportPanel taskId="task/1" />);
    await user.click(screen.getByRole('button', { name: 'ai.generateReport' }));
    expect(await screen.findByRole('button', { name: 'ai.regenerateReport' })).toBeInTheDocument();
    expect(container.querySelector('pre')?.textContent).toBe(
      '<img src=x onerror=alert(1)>\n第一行\n第二行',
    );
    expect(container.querySelector('img')).toBeNull();
    expect(fetchMock.mock.calls[0][0]).toBe(
      'http://localhost:8080/api/v1/ai/report?task_id=task%2F1',
    );
  });
});
