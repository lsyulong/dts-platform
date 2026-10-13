import { describe, it, expect, beforeAll, afterAll, afterEach, beforeEach, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { renderWithProviders, setTestUser } from '../../test/test-utils';
import { server } from '../../test/mocks/server';
import DatasourcesPage from '../datasources';

describe('DatasourcesPage', () => {
  beforeEach(() => setTestUser(['datasource:read', 'datasource:write']));
  beforeAll(() => server.listen());
  afterEach(() => {
    server.resetHandlers();
    vi.restoreAllMocks();
  });
  afterAll(() => server.close());

  function renderPage() {
    return renderWithProviders(<DatasourcesPage />, { route: '/datasources' });
  }

  it('hides datasource creation for accounts with read permission only', async () => {
    setTestUser(['datasource:read']);
    renderPage();
    expect(await screen.findByText('prod-mysql')).toBeInTheDocument();
    expect(screen.queryByText('datasources.addDatasource')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /common.delete/ })).not.toBeInTheDocument();
  });

  it('deletes the selected datasource from its visible table action after confirmation', async () => {
    const user = userEvent.setup();
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const deleted = vi.fn();
    server.use(
      http.delete('http://localhost:8080/api/v1/datasources/:id', ({ params }) => {
        deleted(params.id);
        return HttpResponse.json({ success: true });
      }),
    );
    renderPage();
    await user.click(await screen.findByRole('button', { name: 'common.delete prod-mysql' }));
    await waitFor(() => expect(deleted).toHaveBeenCalledWith('ds-1'));
    expect(window.confirm).toHaveBeenCalledWith('datasources.deleteConfirm');
  });

  it('runs a real connection test from the selected datasource row', async () => {
    const user = userEvent.setup();
    const tested = vi.fn();
    server.use(
      http.post('http://localhost:8080/api/v1/datasources/:id/test', ({ params }) => {
        tested(params.id);
        return HttpResponse.json({
          id: 'probe-1',
          datasource_id: params.id,
          supported: true,
          status: 'healthy',
          latency_ms: 8,
          error_message: null,
          actor_id: 'user-1',
          observed_at: '2026-10-12T08:00:00Z',
        });
      }),
    );
    renderPage();

    await user.click(
      await screen.findByRole('button', {
        name: 'datasources.testConnection prod-mysql',
      }),
    );

    await waitFor(() => expect(tested).toHaveBeenCalledWith('ds-1'));
  });

  it('shows persisted connection test history to read-only users', async () => {
    const user = userEvent.setup();
    setTestUser(['datasource:read']);
    server.use(
      http.get('http://localhost:8080/api/v1/datasources/:id/probes', ({ params }) =>
        HttpResponse.json({
          data: [
            {
              id: 'probe-1',
              datasource_id: params.id,
              supported: false,
              status: 'unknown',
              latency_ms: null,
              error_message: 'unsupported-probe-detail',
              actor_id: 'user-1',
              observed_at: '2026-10-12T08:00:00Z',
            },
          ],
        }),
      ),
    );
    renderPage();

    await user.click(
      await screen.findByRole('button', {
        name: 'datasources.probeHistory prod-mysql',
      }),
    );

    expect(await screen.findByText('unsupported-probe-detail')).toBeInTheDocument();
  });

  describe('loading state', () => {
    it('shows a spinner while loading', () => {
      const { container } = renderPage();
      const skeleton = container.querySelector('.animate-pulse');
      expect(skeleton).toBeInTheDocument();
    });
  });

  describe('loaded state', () => {
    it('renders the page header', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('datasources.title')).toBeInTheDocument();
      });
    });

    it('displays the datasource list', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('prod-mysql')).toBeInTheDocument();
        expect(screen.getByText('prod-pg')).toBeInTheDocument();
        expect(screen.getByText('staging-redis')).toBeInTheDocument();
        expect(screen.getByText('mongo-logs')).toBeInTheDocument();
      });
    });

    it('displays db type and host info for each datasource', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText(/MySQL @ 10.0.1.100:3306/)).toBeInTheDocument();
        expect(screen.getByText(/PostgreSQL @ 10.0.2.50:5432/)).toBeInTheDocument();
      });
    });

    it('displays health summary count of total connections', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('datasources.totalConnections')).toBeInTheDocument();
      });
    });

    it('displays healthy connections count', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('datasources.healthyConnections')).toBeInTheDocument();
      });
    });

    it('displays unhealthy connections count', async () => {
      renderPage();
      await waitFor(() => {
        expect(screen.getByText('datasources.unhealthyConnections')).toBeInTheDocument();
      });
    });
  });

  describe('create datasource modal', () => {
    it('opens the create modal when "datasources.addDatasource" button is clicked', async () => {
      const user = userEvent.setup();
      renderPage();

      await waitFor(() => {
        expect(screen.getByText('prod-mysql')).toBeInTheDocument();
      });

      // Click the "datasources.addDatasource" button in the header
      const addButton = screen.getByRole('button', { name: /datasources.addDatasource/ });
      await user.click(addButton);

      // After click, there should now be TWO elements with "datasources.addDatasource" text
      // (button + modal title), so use getAllByText
      await waitFor(() => {
        const elements = screen.getAllByText('datasources.addDatasource');
        expect(elements.length).toBeGreaterThanOrEqual(2);
      });
    });

    it('renders all form fields in the create modal', async () => {
      const user = userEvent.setup();
      renderPage();

      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByRole('button', { name: /datasources.addDatasource/ }));

      await waitFor(() => {
        expect(screen.getByPlaceholderText('datasources.connectionName')).toBeInTheDocument();
        expect(screen.getByPlaceholderText('datasources.host')).toBeInTheDocument();
        expect(screen.getByPlaceholderText('datasources.username')).toBeInTheDocument();
        expect(screen.getByPlaceholderText('datasources.databaseName')).toBeInTheDocument();
      });
    });

    it('has a cancel button that closes the modal', async () => {
      const user = userEvent.setup();
      renderPage();

      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByRole('button', { name: /datasources.addDatasource/ }));

      await waitFor(() => {
        expect(screen.getByPlaceholderText('datasources.connectionName')).toBeInTheDocument();
      });

      const cancelButton = screen.getByText('common.cancel');
      await user.click(cancelButton);

      // Modal should be removed
      await waitFor(() => {
        expect(screen.queryByPlaceholderText('datasources.connectionName')).not.toBeInTheDocument();
      });
    });

    it('submits the form and closes the modal', async () => {
      const user = userEvent.setup();
      renderPage();

      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByRole('button', { name: /datasources.addDatasource/ }));

      await waitFor(() => {
        expect(screen.getByPlaceholderText('datasources.connectionName')).toBeInTheDocument();
      });

      await user.type(screen.getByPlaceholderText('datasources.connectionName'), 'New MySQL');
      await user.type(screen.getByPlaceholderText('datasources.host'), '192.168.1.1');

      await user.click(screen.getByText('datasources.add'));

      await waitFor(() => {
        expect(screen.queryByPlaceholderText('datasources.connectionName')).not.toBeInTheDocument();
      });
    });
  });
});
