import { test, expect } from '@playwright/test';

test.describe('Datasources', () => {
  test.beforeEach(async ({ page }) => {
    // Login first
    await page.goto('/login');
    await page.getByPlaceholder('default').fill('default');
    await page.getByPlaceholder('admin').fill('admin');
    await page.getByPlaceholder('••••••••').fill('admin123');
    await page.getByRole('button', { name: 'auth.login' }).click();
    await expect(page).toHaveURL('/');
    await page.goto('/datasources');
    await expect(page.locator('h1')).toHaveText('datasources.title');
  });

  test('displays header and add button', async ({ page }) => {
    await expect(page.getByText('datasources.title')).toBeVisible();
    await expect(page.getByText('datasources.subtitle')).toBeVisible();
    await expect(page.getByText('datasources.addDatasource')).toBeVisible();
  });

  test('displays health summary cards', async ({ page }) => {
    await expect(page.getByText('datasources.totalConnections')).toBeVisible();
    await expect(page.getByText('datasources.healthyConnections')).toBeVisible();
    await expect(page.getByText('datasources.unhealthyConnections')).toBeVisible();
  });

  test('displays datasource list from mock data', async ({ page }) => {
    await expect(page.getByText('prod-mysql')).toBeVisible();
    await expect(page.getByText('prod-pg')).toBeVisible();
    await expect(page.getByText('staging-mongo')).toBeVisible();
  });

  test('opens create dialog on add button click', async ({ page }) => {
    await page.getByText('datasources.addDatasource').click();
    await expect(page.getByRole('dialog')).toBeVisible();
    // Dialog title is also "datasources.addDatasource"
    await expect(page.getByPlaceholder('datasources.connectionName')).toBeVisible();
  });

  test('can fill and close create dialog', async ({ page }) => {
    await page.getByText('datasources.addDatasource').click();
    await page.getByPlaceholder('datasources.connectionName').fill('Test DB');
    // Click the close button (X icon) — it's a button without readable text
    await page.getByRole('button', { name: 'Close' }).click();
    await expect(page.getByRole('dialog')).not.toBeVisible();
  });

  test('validates required fields on empty submit', async ({ page }) => {
    await page.getByText('datasources.addDatasource').click();
    // Submit with empty form
    await page.getByRole('button', { name: 'datasources.add' }).click();
    // Toast error should appear
    await expect(page.getByText('required')).toBeVisible();
  });

  test('navigates to datasources via sidebar', async ({ page }) => {
    // Already on datasources page, navigate away and back
    await page.getByText('nav.dashboard').click();
    await expect(page).toHaveURL('/');
    await page.getByText('nav.datasources').click();
    await expect(page).toHaveURL('/datasources');
  });
});
