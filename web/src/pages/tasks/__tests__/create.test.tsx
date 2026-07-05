import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
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
    return render(
      <MemoryRouter initialEntries={['/tasks/create']}>
        <TaskCreatePage />
      </MemoryRouter>
    );
  }

  describe('initial render', () => {
    it('displays all 6 step labels', () => {
      renderPage();
      expect(screen.getByText('基本信息')).toBeInTheDocument();
      expect(screen.getByText('源端配置')).toBeInTheDocument();
      expect(screen.getByText('目标端配置')).toBeInTheDocument();
      expect(screen.getByText('字段映射')).toBeInTheDocument();
      expect(screen.getByText('调度配置')).toBeInTheDocument();
      expect(screen.getByText('确认提交')).toBeInTheDocument();
    });

    it('starts at step 0 with task name input visible', () => {
      renderPage();
      expect(screen.getByPlaceholderText(/MySQL → PostgreSQL/)).toBeInTheDocument();
    });

    it('disables "上一步" button on the first step', () => {
      renderPage();
      const prevButton = screen.getByText('上一步').closest('button');
      expect(prevButton).toBeDisabled();
    });

    it('disables next button when name is empty', () => {
      renderPage();
      const nextButton = screen.getByText('下一步').closest('button');
      expect(nextButton).toBeDisabled();
    });
  });

  describe('step 0: basic info', () => {
    it('enables next button when name is filled', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/MySQL → PostgreSQL/), 'My Task');

      const nextButton = screen.getByText('下一步').closest('button');
      expect(nextButton).not.toBeDisabled();
    });

    it('accepts description input', async () => {
      const user = userEvent.setup();
      renderPage();

      const descInput = screen.getByPlaceholderText(/简要描述/);
      await user.type(descInput, 'This is a test task');

      expect(descInput).toHaveValue('This is a test task');
    });
  });

  describe('step navigation', () => {
    it('navigates to step 1 when clicking next with a name', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/MySQL → PostgreSQL/), 'Test Task');
      await user.click(screen.getByText('下一步'));

      await waitFor(() => {
        expect(screen.getByText('选择数据源端连接')).toBeInTheDocument();
      });
    });

    it('navigates back to previous step', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/MySQL → PostgreSQL/), 'Test Task');
      await user.click(screen.getByText('下一步'));

      await waitFor(() => {
        expect(screen.getByText('选择数据源端连接')).toBeInTheDocument();
      });

      await user.click(screen.getByText('上一步'));

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/MySQL → PostgreSQL/)).toBeInTheDocument();
      });
    });
  });

  describe('step 1: source selection', () => {
    it('shows datasource options on step 1', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/MySQL → PostgreSQL/), 'Test');
      await user.click(screen.getByText('下一步'));

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

      await user.type(screen.getByPlaceholderText(/MySQL → PostgreSQL/), 'Test');
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('prod-mysql'));

      await user.click(screen.getByText('prod-mysql'));
      await user.click(screen.getByText('下一步'));

      await waitFor(() => {
        expect(screen.getByText('选择目标端连接')).toBeInTheDocument();
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
      await user.type(screen.getByPlaceholderText(/MySQL → PostgreSQL/), 'Test');
      // Step 1
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByText('prod-mysql'));
      // Step 2
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('prod-pg'));
      await user.click(screen.getByText('prod-pg'));
      // Step 3
      await user.click(screen.getByText('下一步'));

      await waitFor(() => {
        expect(screen.getByText('字段映射配置')).toBeInTheDocument();
        expect(screen.getByText(/集成 ape-dts 引擎后支持/)).toBeInTheDocument();
      });
    });
  });

  describe('step 4: schedule config', () => {
    it('shows cron input on step 4', async () => {
      const user = userEvent.setup();
      renderPage();

      // Step 0
      await user.type(screen.getByPlaceholderText(/MySQL → PostgreSQL/), 'Test');
      // Step 1
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByText('prod-mysql'));
      // Step 2
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('prod-pg'));
      await user.click(screen.getByText('prod-pg'));
      // Step 3
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('字段映射配置'));
      // Step 4
      await user.click(screen.getByText('下一步'));

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/0 \*\/6 \* \* \*/)).toBeInTheDocument();
      });
    });
  });

  describe('step 5: confirmation', () => {
    it('shows confirmation and submit button on step 5', async () => {
      const user = userEvent.setup();
      renderPage();

      // Step 0
      await user.type(screen.getByPlaceholderText(/MySQL → PostgreSQL/), 'Test Task');
      // Step 1
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByText('prod-mysql'));
      // Step 2
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('prod-pg'));
      await user.click(screen.getByText('prod-pg'));
      // Step 3
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('字段映射配置'));
      // Step 4
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByPlaceholderText(/0 \*\/6 \* \* \*/));
      // Step 5
      await user.click(screen.getByText('下一步'));

      await waitFor(() => {
        expect(screen.getByText('确认任务配置')).toBeInTheDocument();
        expect(screen.getByText('确认创建')).toBeInTheDocument();
      });
    });

    it('displays task name in confirmation summary', async () => {
      const user = userEvent.setup();
      renderPage();

      await user.type(screen.getByPlaceholderText(/MySQL → PostgreSQL/), 'Test Task');
      // Step 1
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('prod-mysql'));
      await user.click(screen.getByText('prod-mysql'));
      // Step 2
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('prod-pg'));
      await user.click(screen.getByText('prod-pg'));
      // Step 3
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByText('字段映射配置'));
      // Step 4
      await user.click(screen.getByText('下一步'));
      await waitFor(() => screen.getByPlaceholderText(/0 \*\/6 \* \* \*/));
      // Step 5
      await user.click(screen.getByText('下一步'));

      await waitFor(() => {
        expect(screen.getByText('确认任务配置')).toBeInTheDocument();
        expect(screen.getByText('确认创建')).toBeInTheDocument();
      });
    });
  });
});
