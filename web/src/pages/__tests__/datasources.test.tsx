import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { server } from '../../test/mocks/server';
import DatasourcesPage from '../datasources';

describe('DatasourcesPage', () => {
  beforeAll(() => server.listen());
  afterEach(() => server.resetHandlers());
  afterAll(() => server.close());

  function renderPage() {
    return render(
      <MemoryRouter>
        <DatasourcesPage />
      </MemoryRouter>
    );
  }

  describe('loading state', () => {
    it('shows a spinner while loading', () => {
      const { container } = renderPage();
      const spinner = container.querySelector('.animate-spin');
      expect(spinner).toBeInTheDocument();
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
