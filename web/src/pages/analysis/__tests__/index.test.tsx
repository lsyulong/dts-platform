import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../../../test/test-utils';
import AnalysisPage from '../index';
import { useAuthStore } from '../../../stores/auth';

const mockPost = vi.fn();
vi.mock('../../../api/client', () => ({
  default: {
    defaults: { baseURL: 'https://configured.example.invalid/api/v1' },
    post: (...args: unknown[]) => mockPost(...args),
  },
}));

function streamedResponse(chunks: string[]) {
  const encoder = new TextEncoder();
  return new Response(
    new ReadableStream<Uint8Array>({
      start(controller) {
        chunks.forEach((chunk) => controller.enqueue(encoder.encode(chunk)));
        controller.close();
      },
    }),
  );
}

async function submitQuestion() {
  const user = userEvent.setup();
  renderWithProviders(<AnalysisPage />);
  await user.type(screen.getByLabelText('analysis.askQuestion'), 'Count connected orders');
  await user.click(screen.getByRole('button', { name: 'analysis.ask' }));
  return user;
}

describe('AnalysisPage', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    localStorage.setItem('token', 'analysis-access-token');
    useAuthStore.setState({ token: 'analysis-access-token' });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it('shows a disabled state without fabricated SQL and prevents repeated requests', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        new Response(JSON.stringify({ message: 'AI 服务未启用' }), { status: 503 }),
      );
    vi.stubGlobal('fetch', fetchMock);
    await submitQuestion();
    expect(await screen.findByRole('status')).toHaveTextContent('analysis.disabled');
    expect(screen.getByRole('button', { name: 'analysis.ask' })).toBeDisabled();
    expect(screen.getByLabelText('analysis.askQuestion')).toBeDisabled();
    expect(screen.queryByText('analysis.generatedSql')).not.toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('expires the shared session when analysis fetch returns unauthorized', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status: 401 })));
    await submitQuestion();
    expect(await screen.findByText('errors.unauthorized')).toBeInTheDocument();
    expect(useAuthStore.getState().token).toBeNull();
    expect(localStorage.getItem('token')).toBeNull();
    expect(mockPost).not.toHaveBeenCalled();
  });

  it('uses the configured API and decodes SQL across fragmented SSE chunks', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        streamedResponse([
          'da',
          'ta: ```sql\\nSELECT 1 AS count;\\n```\\n说明：One row\n\n',
          'data: [DONE]\n\n',
        ]),
      );
    vi.stubGlobal('fetch', fetchMock);
    await submitQuestion();
    expect(await screen.findByText('SELECT 1 AS count;')).toBeInTheDocument();
    expect(screen.getByText('One row')).toBeInTheDocument();
    const url = new URL(fetchMock.mock.calls[0][0]);
    expect(`${url.origin}${url.pathname}`).toBe(
      'https://configured.example.invalid/api/v1/ai/chat',
    );
    expect(url.searchParams.get('message')).toContain('Count connected orders');
    expect(fetchMock.mock.calls[0][1].headers.Authorization).toBe('Bearer analysis-access-token');
  });

  it('preserves stream errors without treating them as executable SQL', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(streamedResponse(['data: [ERROR] Analytics service unavailable\n\n'])),
    );
    await submitQuestion();
    expect(await screen.findByText('Analytics service unavailable')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'analysis.execute' })).not.toBeInTheDocument();
  });

  it('shows actionable query errors and uses a timeout beyond the backend deadline', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(
          streamedResponse(['data: ```sql\\nSELECT 1;\\n```\n\n', 'data: [DONE]\n\n']),
        ),
    );
    mockPost.mockRejectedValue({
      response: { data: { message: 'Result budget exceeded. Add LIMIT.' } },
    });
    const user = await submitQuestion();
    await user.click(await screen.findByRole('button', { name: 'analysis.execute' }));
    expect(await screen.findByText('Result budget exceeded. Add LIMIT.')).toBeInTheDocument();
    expect(mockPost).toHaveBeenCalledWith(
      '/query/execute',
      { sql: 'SELECT 1;' },
      { timeout: 65000 },
    );
  });
});
