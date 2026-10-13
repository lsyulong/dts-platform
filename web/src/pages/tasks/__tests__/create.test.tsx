import { describe, it, expect, beforeAll, afterAll, afterEach, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { renderWithProviders, setTestUser } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import TaskCreatePage from '../create';

describe('TaskCreatePage', () => {
  beforeEach(() => setTestUser(['task:read', 'task:write', 'task:execute']));
  beforeAll(() => server.listen());
  afterEach(() => {
    server.resetHandlers();
    localStorage.clear();
  });
  afterAll(() => server.close());

  function renderPage() {
    return renderWithProviders(<TaskCreatePage />, { route: '/tasks/create' });
  }

  it('does not offer task creation to read-only users', () => {
    setTestUser(['task:read']);
    renderPage();
    expect(screen.getByText('errors.forbidden')).toBeInTheDocument();
    expect(screen.queryByText('tasks.nextStep')).not.toBeInTheDocument();
  });

  describe('initial render', () => {
    it('displays all 6 step labels', () => {
      renderPage();
      expect(screen.getByText('tasks.steps.basic')).toBeInTheDocument();
      expect(screen.getByText('tasks.steps.source')).toBeInTheDocument();
      expect(screen.getByText('tasks.steps.target')).toBeInTheDocument();
      expect(screen.getByText('tasks.steps.mapping')).toBeInTheDocument();
      expect(screen.getByText('tasks.steps.schedule')).toBeInTheDocument();
      expect(screen.getByText('tasks.steps.confirm')).toBeInTheDocument();
    });

    it('starts at step 0 with task name input visible', () => {
      renderPage();
      expect(screen.getByPlaceholderText(/tasks.namePlaceholder/)).toBeInTheDocument();
    });

    it('disables "tasks.prevStep" button on the first step', () => {
      renderPage();
      const prevButton = screen.getByText('tasks.prevStep').closest('button');
      expect(prevButton).toBeDisabled();
    });

    it('disables next button when name is empty', () => {
      renderPage();
      const nextButton = screen.getByText('tasks.nextStep').closest('button');
      expect(nextButton).toBeDisabled();
    });
  });

  describe('step 0: basic info', () => {
    it('enables next button when name is filled', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'My Task');

      const nextButton = screen.getByText('tasks.nextStep').closest('button');
      expect(nextButton).not.toBeDisabled();
    });

    it('accepts description input', async () => {
      const user = userEvent.setup();
      renderPage();

      const descInput = screen.getByPlaceholderText(/tasks.descPlaceholder/);
      await user.type(descInput, 'This is a test task');

      expect(descInput).toHaveValue('This is a test task');
    });
  });

  describe('step navigation', () => {
    it('navigates to step 1 when clicking next with a name', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Test Task');
      await user.click(screen.getByText('tasks.nextStep'));

      await waitFor(() => {
        expect(screen.getByText('tasks.selectSource')).toBeInTheDocument();
      });
    });

    it('navigates back to previous step', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Test Task');
      await user.click(screen.getByText('tasks.nextStep'));

      await waitFor(() => {
        expect(screen.getByText('tasks.selectSource')).toBeInTheDocument();
      });

      await user.click(screen.getByText('tasks.prevStep'));

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/tasks.namePlaceholder/)).toBeInTheDocument();
      });
    });
  });

  describe('step 1: source selection', () => {
    it('shows datasource options on step 1', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Test');
      await user.click(screen.getByText('tasks.nextStep'));

      await waitFor(() => {
        expect(screen.getByText('prod-mysql')).toBeInTheDocument();
        expect(screen.getByText('prod-pg')).toBeInTheDocument();
        expect(screen.queryByText('staging-redis')).not.toBeInTheDocument();
        expect(screen.queryByText('mongo-logs')).not.toBeInTheDocument();
      });
    });
  });

  describe('step 2: target selection', () => {
    it('shows filtered datasources (excluding selected source)', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Test');
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('prod-mysql'));

      await user.click(screen.getByText('prod-mysql'));
      await user.click(screen.getByText('tasks.nextStep'));

      await waitFor(() => {
        expect(screen.getByText('tasks.selectTarget')).toBeInTheDocument();
        expect(screen.queryByText('prod-mysql')).not.toBeInTheDocument();
        expect(screen.getByText('prod-pg')).toBeInTheDocument();
      });
    });

    it('offers only DuckDB targets when the source is DuckDB', async () => {
      server.use(
        http.get('http://localhost:8080/api/v1/datasources', () =>
          HttpResponse.json({
            data: [
              {
                id: 'mysql-1',
                name: 'source-mysql',
                db_type: 'MySQL',
                host: 'mysql',
                port: 3306,
                username: 'reader',
                database_name: 'sales',
                health_status: 'healthy',
                last_test_at: null,
                created_at: '2026-01-01T00:00:00Z',
              },
              {
                id: 'duck-1',
                name: 'source-duckdb',
                db_type: 'DuckDB',
                host: 'swanlake',
                port: 4214,
                username: null,
                database_name: 'source_lake',
                health_status: 'healthy',
                last_test_at: null,
                created_at: '2026-01-01T00:00:00Z',
              },
              {
                id: 'duck-2',
                name: 'target-duckdb',
                db_type: 'DuckDB',
                host: 'swanlake',
                port: 4214,
                username: null,
                database_name: 'warehouse',
                health_status: 'healthy',
                last_test_at: null,
                created_at: '2026-01-01T00:00:00Z',
              },
            ],
          }),
        ),
      );
      const user = userEvent.setup();
      renderPage();
      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Lake copy');
      await user.click(screen.getByText('tasks.nextStep'));
      await user.click(await screen.findByText('source-duckdb'));
      await user.click(screen.getByText('tasks.nextStep'));
      expect(await screen.findByText('target-duckdb')).toBeInTheDocument();
      expect(screen.queryByText('source-mysql')).not.toBeInTheDocument();
    });
  });

  describe('step 3: table mapping', () => {
    it('requires source and target tables before proceeding', async () => {
      const user = userEvent.setup();
      renderPage();

      // Fill name
      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Test');
      // Step 1
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByText('prod-mysql'));
      // Step 2
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('prod-pg'));
      await user.click(screen.getByText('prod-pg'));
      // Step 3
      await user.click(screen.getByText('tasks.nextStep'));

      await waitFor(() => {
        expect(screen.getByText('tasks.mappingConfig')).toBeInTheDocument();
        expect(screen.getByText(/tasks.mappingHint/)).toBeInTheDocument();
      });
      expect(screen.getByText('tasks.nextStep').closest('button')).toBeDisabled();
      await user.type(screen.getByLabelText(/tasks.mapping.sourceTable/), 'customers');
      expect(screen.getByText('tasks.nextStep').closest('button')).toBeDisabled();
      await user.type(screen.getByLabelText(/tasks.mapping.targetTable/), 'customers');
      expect(screen.getByText('tasks.nextStep').closest('button')).not.toBeDisabled();
    });

    it('creates a mapping from the discovered source catalog', async () => {
      server.use(
        http.get('http://localhost:8080/api/v1/datasources/:id/tables', () =>
          HttpResponse.json({ data: [{ schema: 'sales', name: 'customers' }] }),
        ),
      );
      const user = userEvent.setup();
      renderPage();
      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Catalog task');
      await user.click(screen.getByText('tasks.nextStep'));
      await user.click(await screen.findByText('prod-mysql'));
      await user.click(screen.getByText('tasks.nextStep'));
      await user.click(screen.getByText('prod-pg'));
      await user.click(screen.getByText('tasks.nextStep'));
      const catalog = await screen.findByLabelText('tasks.mapping.discoveredTable');
      await user.selectOptions(catalog, 'sales.customers');
      expect(screen.getByLabelText(/tasks.mapping.sourceSchema/)).toHaveValue('sales');
      expect(screen.getByLabelText(/tasks.mapping.sourceTable/)).toHaveValue('customers');
      expect(screen.getByLabelText(/tasks.mapping.targetTable/)).toHaveValue('customers');
    });
  });

  describe('step 4: schedule config', () => {
    it('shows manual execution mode on step 4', async () => {
      const user = userEvent.setup();
      renderPage();

      // Step 0
      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Test');
      // Step 1
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByText('prod-mysql'));
      // Step 2
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('prod-pg'));
      await user.click(screen.getByText('prod-pg'));
      // Step 3
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('tasks.mappingConfig'));
      await user.type(screen.getByLabelText(/tasks.mapping.sourceTable/), 'customers');
      await user.type(screen.getByLabelText(/tasks.mapping.targetTable/), 'customers');
      // Step 4
      await user.click(screen.getByText('tasks.nextStep'));

      await waitFor(() => {
        expect(screen.getByText('tasks.manualTrigger')).toBeInTheDocument();
      });
    });
  });

  describe('step 5: confirmation', () => {
    it('shows confirmation and submit button on step 5', async () => {
      const user = userEvent.setup();
      renderPage();

      // Step 0
      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Test Task');
      // Step 1
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByText('prod-mysql'));
      // Step 2
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('prod-pg'));
      await user.click(screen.getByText('prod-pg'));
      // Step 3
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('tasks.mappingConfig'));
      await user.type(screen.getByLabelText(/tasks.mapping.sourceTable/), 'customers');
      await user.type(screen.getByLabelText(/tasks.mapping.targetTable/), 'customers');
      // Step 4
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('tasks.manualTrigger'));
      // Step 5
      await user.click(screen.getByText('tasks.nextStep'));

      await waitFor(() => {
        expect(screen.getByText('tasks.confirmTitle')).toBeInTheDocument();
        expect(screen.getByText('tasks.confirmCreate')).toBeInTheDocument();
      });
    });

    it('displays task name in confirmation summary', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Test Task');
      // Step 1
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByText('prod-mysql'));
      // Step 2
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('prod-pg'));
      await user.click(screen.getByText('prod-pg'));
      // Step 3
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('tasks.mappingConfig'));
      await user.type(screen.getByLabelText(/tasks.mapping.sourceTable/), 'customers');
      await user.type(screen.getByLabelText(/tasks.mapping.targetTable/), 'customers');
      // Step 4
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByText('tasks.manualTrigger'));
      // Step 5
      await user.click(screen.getByText('tasks.nextStep'));

      await waitFor(() => {
        expect(screen.getByText('tasks.confirmTitle')).toBeInTheDocument();
        expect(screen.getByText('tasks.confirmCreate')).toBeInTheDocument();
      });
    });
  });

  it('submits trimmed table mappings, full sync mode and batch size after confirmation', async () => {
    let submitted: unknown;
    server.use(
      http.post('http://localhost:8080/api/v1/tasks', async ({ request }) => {
        submitted = await request.json();
        return HttpResponse.json({ id: 'new-task' }, { status: 201 });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.type(screen.getByPlaceholderText(/tasks.namePlaceholder/), 'Customer sync');
    await user.click(screen.getByText('tasks.nextStep'));
    await user.click(await screen.findByText('prod-mysql'));
    await user.click(screen.getByText('tasks.nextStep'));
    await user.click(screen.getByText('prod-pg'));
    await user.click(screen.getByText('tasks.nextStep'));
    await user.type(screen.getByLabelText('tasks.mapping.sourceSchema'), ' sales ');
    await user.type(screen.getByLabelText(/tasks.mapping.sourceTable/), ' customers ');
    await user.type(screen.getByLabelText('tasks.mapping.targetSchema'), ' public ');
    await user.type(screen.getByLabelText(/tasks.mapping.targetTable/), ' customers ');
    await user.type(screen.getByLabelText('tasks.mapping.batchSize'), '500');
    await user.click(screen.getByText('tasks.nextStep'));
    await user.click(screen.getByText('tasks.nextStep'));
    expect(screen.getByText('sales.customers → public.customers')).toBeInTheDocument();
    expect(screen.getByText('tasks.mapping.full')).toBeInTheDocument();
    await user.click(screen.getByText('tasks.confirmCreate'));
    await waitFor(() =>
      expect(submitted).toEqual({
        name: 'Customer sync',
        description: '',
        source_id: 'ds-1',
        target_id: 'ds-2',
        cron_expr: null,
        sync_mode: 'full',
        batch_size: 500,
        table_mappings: [
          {
            source_table: 'customers',
            target_table: 'customers',
            source_schema: 'sales',
            target_schema: 'public',
          },
        ],
      }),
    );
  });
});
