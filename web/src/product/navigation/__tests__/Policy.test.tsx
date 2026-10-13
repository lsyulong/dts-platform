import { act, cleanup, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { useAuthStore } from '../../../stores/auth';
import { createTestQueryClient, renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import { NavigationProvider, useNavigation } from '..';
import { API, gate, login, policy } from './fixtures';

function Probe() {
  const navigation = useNavigation();
  return (
    <>
      <p data-testid="phase">{navigation.phase}</p>
      <p data-testid="nodes">{navigation.nodes.map((node) => node.name).join(',')}</p>
      <p data-testid="detail">{String(navigation.allowsPath('/warehouse/models/record-a'))}</p>
      <p data-testid="sibling">{String(navigation.allowsPath('/warehouse/models-secret'))}</p>
      <p data-testid="unregistered">{String(navigation.allowsPath('/system/users'))}</p>
      <button onClick={() => void navigation.refresh()}>refresh</button>
    </>
  );
}
const renderPolicy = () =>
  renderWithProviders(
    <NavigationProvider>
      <Probe />
    </NavigationProvider>,
  );

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => login());
afterEach(() => {
  cleanup();
  server.resetHandlers();
  act(() =>
    useAuthStore.setState({
      user: null,
      token: null,
      initialized: true,
      restoring: false,
      restoreError: null,
    }),
  );
  localStorage.clear();
});

describe('navigation policy isolation', () => {
  it('retains an already verified policy during a background refresh so mounted forms survive', async () => {
    const pending = gate();
    let reads = 0;
    server.use(
      http.get(`${API}/product/navigation/current`, async () => {
        if (++reads > 1) await pending.promise;
        return HttpResponse.json(policy());
      }),
    );
    renderPolicy();
    await screen.findByText('真实模型目录');
    await userEvent.click(screen.getByRole('button', { name: 'refresh' }));
    await waitFor(() => expect(reads).toBe(2));
    expect(screen.getByTestId('phase')).toHaveTextContent('ready');
    expect(screen.getByTestId('detail')).toHaveTextContent('true');
    await act(async () => {
      pending.release();
      await pending.promise;
    });
    expect(screen.getByTestId('nodes')).toHaveTextContent('真实模型目录');
  });
  it('hides the previous policy during a permission change and ignores the old request', async () => {
    const pending = gate();
    const observed: string[] = [];
    server.use(
      http.get(`${API}/product/navigation/current`, async ({ request }) => {
        const token = request.headers.get('authorization') ?? '';
        observed.push(token);
        if (token === 'Bearer navigation-session') {
          await pending.promise;
          return HttpResponse.json(policy());
        }
        return HttpResponse.json(policy({ nodes: [], filtered_count: 1 }));
      }),
    );
    const cache = createTestQueryClient();
    renderWithProviders(
      <NavigationProvider>
        <Probe />
      </NavigationProvider>,
      { queryClient: cache },
    );
    await waitFor(() => expect(observed).toHaveLength(1));
    act(() => login(['system:read'], 'replacement-token'));
    expect(screen.getByTestId('nodes')).toHaveTextContent('');
    await waitFor(() => expect(screen.getByTestId('phase')).toHaveTextContent('ready'));
    await act(async () => {
      pending.release();
      await pending.promise;
    });
    expect(screen.getByTestId('nodes')).toHaveTextContent('');
    expect(screen.getByTestId('detail')).toHaveTextContent('false');
    expect(observed).toEqual(['Bearer navigation-session', 'Bearer replacement-token']);
    expect(
      JSON.stringify(
        cache
          .getQueryCache()
          .getAll()
          .map((query) => query.queryKey),
      ),
    ).not.toContain('replacement-token');
  });

  it('clears cached nodes while session restoration is pending without sending a new request', async () => {
    let reads = 0;
    server.use(
      http.get(`${API}/product/navigation/current`, () => {
        reads++;
        return HttpResponse.json(policy());
      }),
    );
    renderPolicy();
    await screen.findByText('真实模型目录');
    act(() => useAuthStore.setState({ restoring: true }));
    expect(screen.getByTestId('phase')).toHaveTextContent('loading');
    expect(screen.getByTestId('nodes')).toHaveTextContent('');
    expect(reads).toBe(1);
  });

  it('fails closed after a refresh error rather than showing cached or default routes', async () => {
    let reads = 0;
    server.use(
      http.get(`${API}/product/navigation/current`, () =>
        ++reads === 1
          ? HttpResponse.json(policy())
          : HttpResponse.json({ message: 'unavailable' }, { status: 503 }),
      ),
    );
    renderPolicy();
    await screen.findByText('真实模型目录');
    await userEvent.click(screen.getByRole('button', { name: 'refresh' }));
    await waitFor(() => expect(screen.getByTestId('phase')).toHaveTextContent('error'));
    expect(screen.getByTestId('nodes')).toHaveTextContent('');
    expect(screen.getByTestId('detail')).toHaveTextContent('false');
  });

  it('uses only the returned expired policy and matches descendant routes at a path boundary', async () => {
    let expired = false;
    server.use(
      http.get(`${API}/product/navigation/current`, () =>
        HttpResponse.json(expired ? policy({ mode: 'expired', nodes: [] }) : policy()),
      ),
    );
    renderPolicy();
    await screen.findByText('真实模型目录');
    expect(screen.getByTestId('detail')).toHaveTextContent('true');
    expect(screen.getByTestId('sibling')).toHaveTextContent('false');
    expect(screen.getByTestId('unregistered')).toHaveTextContent('false');
    expired = true;
    await userEvent.click(screen.getByRole('button', { name: 'refresh' }));
    await waitFor(() => expect(screen.getByTestId('phase')).toHaveTextContent('ready'));
    expect(screen.getByTestId('nodes')).toHaveTextContent('');
    expect(screen.getByTestId('detail')).toHaveTextContent('false');
  });

  it.each(['foreign tenant', 'external path'])('rejects a response with %s', async (invalid) => {
    const response = policy();
    server.use(
      http.get(`${API}/product/navigation/current`, () =>
        HttpResponse.json(
          invalid === 'foreign tenant'
            ? { ...response, tenant_id: 'other-tenant' }
            : { ...response, nodes: [{ ...response.nodes[0], path: '//evil.example' }] },
        ),
      ),
    );
    renderPolicy();
    await waitFor(() => expect(screen.getByTestId('phase')).toHaveTextContent('error'));
    expect(screen.getByTestId('nodes')).toHaveTextContent('');
  });
});
