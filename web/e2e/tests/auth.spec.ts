import { test, expect } from '@playwright/test';

test.describe('Login', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/login');
  });

  test('renders login form with app branding', async ({ page }) => {
    await expect(page.locator('h1')).toHaveText('common.appName');
    await expect(page.getByPlaceholder('default')).toBeVisible();
    await expect(page.getByPlaceholder('admin')).toBeVisible();
    await expect(page.getByPlaceholder('••••••••')).toBeVisible();
    await expect(page.getByRole('button', { name: 'auth.login' })).toBeVisible();
  });

  test('redirects to dashboard on successful login', async ({ page }) => {
    await page.getByPlaceholder('default').fill('default');
    await page.getByPlaceholder('admin').fill('admin');
    await page.getByPlaceholder('••••••••').fill('admin123');
    await page.getByRole('button', { name: 'auth.login' }).click();

    // Should redirect to dashboard
    await expect(page).toHaveURL('/');
    await expect(page.locator('h1')).toContainText('dashboard.title');
  });

  test('shows error on wrong credentials', async ({ page }) => {
    await page.getByPlaceholder('default').fill('default');
    await page.getByPlaceholder('admin').fill('wrong');
    await page.getByPlaceholder('••••••••').fill('wrong');
    await page.getByRole('button', { name: 'auth.login' }).click();

    // Error message should appear (from i18n mock or real translation)
    await expect(page.getByText('auth.loginFailed')).toBeVisible();
  });

  test('redirects to login if not authenticated', async ({ page }) => {
    // Clear localStorage to simulate logged-out state
    await page.evaluate(() => localStorage.clear());
    await page.goto('/tasks');
    await expect(page).toHaveURL('/login');
  });
});
