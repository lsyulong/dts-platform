import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('token', 'browser-test-token');
    localStorage.setItem('i18nextLng', 'zh-CN');
    localStorage.setItem(
      'user',
      JSON.stringify({
        id: 'test-user',
        username: 'test',
        tenant_id: 'test-tenant',
        roles: ['tenant_admin'],
        permissions: [
          'task:read',
          'task:write',
          'task:execute',
          'datasource:read',
          'datasource:write',
        ],
      }),
    );
  });
  await page.route('**/api/v1/auth/me', (route) =>
    route.fulfill({
      json: {
        id: 'test-user',
        username: 'test',
        email: 'test@example.invalid',
        tenant_id: 'test-tenant',
        tenant_name: '测试租户',
        roles: ['tenant_admin'],
        permissions: [
          'task:read',
          'task:write',
          'task:execute',
          'datasource:read',
          'datasource:write',
        ],
      },
    }),
  );
  await page.route('**/api/v1/datasources', (route) =>
    route.fulfill({
      json: {
        data: [
          {
            id: 'source-1',
            name: '业务库',
            db_type: 'MySQL',
            host: 'source',
            port: 3306,
            database_name: 'sales',
            health_status: 'healthy',
          },
          {
            id: 'target-1',
            name: '分析库',
            db_type: 'PostgreSQL',
            host: 'target',
            port: 5432,
            database_name: 'warehouse',
            health_status: 'healthy',
          },
        ],
      },
    }),
  );
});

test('creates a pipeline with mappings through the translated wizard', async ({
  page,
}, testInfo) => {
  let submitted: Record<string, unknown> | undefined;
  await page.route('**/api/v1/tasks', async (route) => {
    if (route.request().method() === 'POST') {
      submitted = route.request().postDataJSON();
      await route.fulfill({ status: 201, json: { id: 'new-pipeline' } });
    } else await route.fulfill({ json: { data: [] } });
  });
  await page.goto('/tasks/create');
  await page.getByLabel('任务名称').fill('客户数据全量同步');
  await page.getByRole('button', { name: '下一步' }).click();
  await page.getByText('业务库', { exact: true }).click();
  await page.getByRole('button', { name: '下一步' }).click();
  await page.getByText('分析库', { exact: true }).click();
  await page.getByRole('button', { name: '下一步' }).click();
  await expect(page.getByRole('button', { name: '下一步' })).toBeDisabled();
  await page.getByLabel('源 Schema（可选）').fill('sales');
  await page.getByLabel('目标 Schema（可选）').fill('public');
  await page.getByRole('textbox', { name: '源表', exact: true }).fill('customers');
  await page.getByRole('textbox', { name: '目标表', exact: true }).fill('customers');
  await page.getByLabel('批次行数（可选）').fill('500');
  await page.screenshot({ path: testInfo.outputPath('pipeline-mapping.png'), fullPage: true });
  await page.getByRole('button', { name: '下一步' }).click();
  await page.getByRole('button', { name: '下一步' }).click();
  await expect(page.getByText('sales.customers → public.customers')).toBeVisible();
  await page.getByRole('button', { name: '确认创建' }).click();
  await expect(page).toHaveURL('/tasks');
  expect(submitted).toMatchObject({
    sync_mode: 'full',
    batch_size: 500,
    table_mappings: [
      {
        source_schema: 'sales',
        target_schema: 'public',
        source_table: 'customers',
        target_table: 'customers',
      },
    ],
  });
});

test('shows a cancellation failure without pretending the run stopped', async ({ page }) => {
  await page.route('**/api/v1/tasks/task-1', (route) =>
    route.fulfill({
      json: {
        id: 'task-1',
        name: '客户同步',
        source_name: '业务库',
        source_type: 'MySQL',
        target_name: '分析库',
        target_type: 'PostgreSQL',
        status: 'running',
        config_json: '{}',
        created_at: '2026-10-08',
        recent_runs: [],
      },
    }),
  );
  await page.route('**/api/v1/tasks/task-1/schedule', (route) =>
    route.fulfill({ json: { has_timer: false, schedule_history: [] } }),
  );
  await page.route('**/api/v1/tasks/task-1/status', (route) => {
    expect(route.request().postDataJSON()).toEqual({ status: 'cancelled' });
    return route.fulfill({ status: 503, json: { message: '采集进程暂时不可用' } });
  });
  await page.goto('/tasks/task-1');
  await page.getByRole('button', { name: '取消运行' }).click();
  await expect(page.getByText('更新任务状态失败: 采集进程暂时不可用')).toBeVisible();
  await expect(page.getByText('运行中', { exact: true })).toBeVisible();
});
