import { test, expect } from '@playwright/test';

test.describe('Tasks', () => {
  test.beforeEach(async ({ page }) => {
    // Login first
    await page.goto('/login');
    await page.getByPlaceholder('default').fill('default');
    await page.getByPlaceholder('admin').fill('admin');
    await page.getByPlaceholder('••••••••').fill('admin123');
    await page.getByRole('button', { name: 'auth.login' }).click();
    await expect(page).toHaveURL('/');
  });

  test.describe('Task List', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/tasks');
      await expect(page.locator('h1')).toHaveText('tasks.title');
    });

    test('displays all 5 mock tasks', async ({ page }) => {
      await expect(page.getByText('MySQL to PostgreSQL Sync')).toBeVisible();
      await expect(page.getByText('MongoDB to ClickHouse')).toBeVisible();
      await expect(page.getByText('Kafka to Redis Cache')).toBeVisible();
      await expect(page.getByText('Oracle Migration')).toBeVisible();
      await expect(page.getByText('Completed Backup Sync')).toBeVisible();
    });

    test('filters tasks by search input', async ({ page }) => {
      const searchInput = page.getByPlaceholder(/tasks.searchPlaceholder/);
      await searchInput.fill('MySQL');
      await expect(page.getByText('MySQL to PostgreSQL Sync')).toBeVisible();
      await expect(page.getByText('Kafka to Redis Cache')).not.toBeVisible();
    });

    test('shows empty state when search matches nothing', async ({ page }) => {
      const searchInput = page.getByPlaceholder(/tasks.searchPlaceholder/);
      await searchInput.fill('NONEXISTENT');
      await expect(page.getByText('tasks.noTasks')).toBeVisible();
      await expect(page.getByText('tasks.createFirst →')).toBeVisible();
    });

    test('has a link to create task page', async ({ page }) => {
      await page.getByText('tasks.createTask').click();
      await expect(page).toHaveURL('/tasks/create');
    });

    test('navigates to task detail on click', async ({ page }) => {
      await page.getByText('MySQL to PostgreSQL Sync').click();
      await expect(page).toHaveURL(/\/tasks\/task-1/);
      await expect(page.getByText('MySQL to PostgreSQL Sync')).toBeVisible();
    });
  });

  test.describe('Task Create Wizard', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/tasks/create');
    });

    test('displays all 6 step labels', async ({ page }) => {
      for (const step of [
        'tasks.steps.basic',
        'tasks.steps.source',
        'tasks.steps.target',
        'tasks.steps.mapping',
        'tasks.steps.schedule',
        'tasks.steps.confirm',
      ]) {
        await expect(page.getByText(step)).toBeVisible();
      }
    });

    test('next button is disabled when name is empty', async ({ page }) => {
      const nextBtn = page.getByRole('button', { name: 'tasks.nextStep' });
      await expect(nextBtn).toBeDisabled();
    });

    test('can navigate through steps', async ({ page }) => {
      // Step 0: fill name
      await page.getByPlaceholder('tasks.namePlaceholder').fill('E2E Test Task');
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();

      // Step 1: select source
      await expect(page.getByText('tasks.selectSource')).toBeVisible();
      await page.getByText('prod-mysql').click();
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();

      // Step 2: select target
      await expect(page.getByText('tasks.selectTarget')).toBeVisible();
      await page.getByText('prod-pg').click();
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();

      // Step 3: mapping placeholder
      await expect(page.getByText('tasks.mappingConfig')).toBeVisible();
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();

      // Step 4: schedule
      await expect(page.getByPlaceholder('tasks.cronPlaceholder')).toBeVisible();
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();

      // Step 5: confirm
      await expect(page.getByText('tasks.confirmTitle')).toBeVisible();
      await expect(page.getByText('E2E Test Task')).toBeVisible();
    });

    test('prev button navigates backward', async ({ page }) => {
      await page.getByPlaceholder('tasks.namePlaceholder').fill('test');
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();
      await page.getByRole('button', { name: 'tasks.prevStep' }).click();
      await expect(page.getByPlaceholder('tasks.namePlaceholder')).toHaveValue('test');
    });

    test('can complete full creation flow', async ({ page }) => {
      // Step 0
      await page.getByPlaceholder('tasks.namePlaceholder').fill('E2E Created Task');
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();
      // Step 1
      await page.getByText('prod-mysql').click();
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();
      // Step 2
      await page.getByText('prod-pg').click();
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();
      // Step 3
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();
      // Step 4
      await page.getByRole('button', { name: 'tasks.nextStep' }).click();
      // Step 5
      await expect(page.getByText('tasks.confirmCreate')).toBeVisible();
    });
  });

  test.describe('Task Detail', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/tasks/task-1');
      await expect(page.locator('h1')).toContainText('MySQL to PostgreSQL Sync');
    });

    test('displays task info', async ({ page }) => {
      await expect(page.getByText('MySQL to PostgreSQL Sync')).toBeVisible();
      await expect(page.getByText('MySQL → PostgreSQL')).toBeVisible();
    });

    test('shows execution history', async ({ page }) => {
      await expect(page.getByText('tasks.detail.executionHistory')).toBeVisible();
    });

    test('has back navigation to task list', async ({ page }) => {
      await page.getByRole('link', { name: '' }).first().click();
      await expect(page).toHaveURL('/tasks');
    });
  });
});
