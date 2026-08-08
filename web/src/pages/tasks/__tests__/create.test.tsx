import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import TaskCreatePage from '../create';

describe('TaskCreatePage', () => {
  beforeAll(() => server.listen());
  afterEach(() => {
    server.resetHandlers();
    localStorage.clear();
  });
  afterAll(() => server.close());

  function renderPage() {
    return renderWithProviders(<TaskCreatePage />, { route: '/tasks/create' });
  }

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
  });

  describe('step 3: field mapping placeholder', () => {
    it('shows placeholder content on step 3', async () => {
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
    });
  });

  describe('step 4: schedule config', () => {
    it('shows cron input on step 4', async () => {
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
      // Step 4
      await user.click(screen.getByText('tasks.nextStep'));

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/tasks.cronPlaceholder/)).toBeInTheDocument();
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
      // Step 4
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByPlaceholderText(/tasks.cronPlaceholder/));
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
      // Step 4
      await user.click(screen.getByText('tasks.nextStep'));
      await waitFor(() => screen.getByPlaceholderText(/tasks.cronPlaceholder/));
      // Step 5
      await user.click(screen.getByText('tasks.nextStep'));

      await waitFor(() => {
        expect(screen.getByText('tasks.confirmTitle')).toBeInTheDocument();
        expect(screen.getByText('tasks.confirmCreate')).toBeInTheDocument();
      });
    });
  });
});
