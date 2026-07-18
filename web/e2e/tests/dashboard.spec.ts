import { test, expect } from '@playwright/test';

test.describe('Dashboard', () => {
  test.beforeEach(async ({ page }) => {
    // Login first
    await page.goto('/login');
    await page.getByPlaceholder('default').fill('default');
    await page.getByPlaceholder('admin').fill('admin');
    await page.getByPlaceholder('••••••••').fill('admin123');
    await page.getByRole('button', { name: 'auth.login' }).click();
    await expect(page).toHaveURL('/');
  });

  test('displays dashboard title and subtitle', async ({ page }) => {
    await expect(page.locator('h1')).toHaveText('dashboard.title');
    await expect(page.getByText('dashboard.subtitle')).toBeVisible();
  });

  test('displays all four KPI cards', async ({ page }) => {
    await expect(page.getByText('dashboard.totalTasks')).toBeVisible();
    await expect(page.getByText('dashboard.todaySyncData')).toBeVisible();
    await expect(page.getByText('dashboard.successRate')).toBeVisible();
    await expect(page.getByText('dashboard.healthyDatasources')).toBeVisible();
  });

  test('displays success rate 95.8% from mock data', async ({ page }) => {
    await expect(page.getByText('95.8%')).toBeVisible();
  });

  test('displays healthy datasources 7/8', async ({ page }) => {
    await expect(page.getByText('7/8')).toBeVisible();
  });

  test('displays recent runs section with task names', async ({ page }) => {
    await expect(page.getByText('dashboard.recentRuns')).toBeVisible();
    await expect(page.getByText('MySQL to PG Sync')).toBeVisible();
    await expect(page.getByText('Mongo to CH')).toBeVisible();
  });

  test('has a link to view all tasks', async ({ page }) => {
    const viewAllLink = page.getByText('dashboard.viewAll');
    await expect(viewAllLink).toBeVisible();
    await expect(viewAllLink).toHaveAttribute('href', '/tasks');
  });

  test('displays chart sections', async ({ page }) => {
    await expect(page.getByText('dashboard.syncVolumeChart')).toBeVisible();
    await expect(page.getByText('dashboard.taskDistribution')).toBeVisible();
  });

  test('displays system log section', async ({ page }) => {
    await expect(page.getByText('dashboard.systemLog')).toBeVisible();
    await expect(page.getByText('dashboard.logStarted')).toBeVisible();
  });

  test('navigates to tasks via sidebar', async ({ page }) => {
    await page.getByText('nav.tasks').click();
    await expect(page).toHaveURL('/tasks');
    await expect(page.locator('h1')).toHaveText('tasks.title');
  });
});
