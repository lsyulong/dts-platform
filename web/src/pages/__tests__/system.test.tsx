import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders, setTestUser } from '../../test/test-utils';
import SystemSettingsPage from '../system';

const mockGet = vi.fn();
const mockPut = vi.fn();
vi.mock('../../api/client', () => ({
  default: {
    get: (...args: unknown[]) => mockGet(...args),
    put: (...args: unknown[]) => mockPut(...args),
  },
}));

const managedUser = {
  id: 'managed-user',
  username: 'alice',
  email: 'alice@example.invalid',
  display_name: 'Alice',
  tenant_id: 'test-tenant',
  tenant_name: 'Current tenant',
  roles: ['analyst'],
  status: 'active',
  created_at: '2026-10-08',
};

function mockUsersAndRoles(roles: Array<{ id: string; name: string; tenant_id: string }>) {
  mockGet.mockImplementation((url: string) =>
    Promise.resolve({ data: { data: url.endsWith('/users') ? [managedUser] : roles } }),
  );
}

describe('SystemSettingsPage', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    setTestUser(['user:manage']);
    mockGet.mockResolvedValue({ data: { data: [] } });
  });

  it('blocks the management page before requesting admin data for a read-only account', () => {
    setTestUser(['task:read']);
    renderWithProviders(<SystemSettingsPage />);
    expect(screen.getByText('errors.forbidden')).toBeInTheDocument();
    expect(mockGet).not.toHaveBeenCalled();
  });

  it('shows only the current tenant in the tenant table', async () => {
    mockGet.mockImplementation((url: string) =>
      Promise.resolve({
        data: {
          data: url.endsWith('/tenants')
            ? [
                {
                  id: 'test-tenant',
                  name: 'Current tenant',
                  slug: 'current',
                  status: 'active',
                  created_at: '2026-10-08',
                },
                {
                  id: 'other-tenant',
                  name: 'Other tenant',
                  slug: 'other',
                  status: 'active',
                  created_at: '2026-10-08',
                },
              ]
            : [],
        },
      }),
    );
    const user = userEvent.setup();
    renderWithProviders(<SystemSettingsPage />);
    await user.click(screen.getByRole('button', { name: 'system.tenants' }));
    expect(await screen.findByText('Current tenant')).toBeInTheDocument();
    expect(screen.queryByText('Other tenant')).not.toBeInTheDocument();
  });

  it('does not offer editing or deletion of a system role', async () => {
    mockGet.mockImplementation((url: string) =>
      Promise.resolve({
        data: {
          data: url.endsWith('/roles')
            ? [
                {
                  id: 'system-role',
                  name: 'tenant_admin',
                  is_system: true,
                  description: 'Built-in',
                  permissions: ['user:manage'],
                },
              ]
            : [],
        },
      }),
    );
    const user = userEvent.setup();
    renderWithProviders(<SystemSettingsPage />);
    await user.click(screen.getByRole('button', { name: 'system.roles' }));
    expect(await screen.findByText('tenant_admin')).toBeInTheDocument();
    expect(screen.queryByTitle('common.edit')).not.toBeInTheDocument();
    expect(screen.queryByTitle('common.delete')).not.toBeInTheDocument();
  });

  it('preserves an existing role by submitting its ID when only the display name changes', async () => {
    mockUsersAndRoles([{ id: 'role-analyst-id', name: 'analyst', tenant_id: 'test-tenant' }]);
    mockPut.mockResolvedValue({ data: {} });
    const user = userEvent.setup();
    renderWithProviders(<SystemSettingsPage />);
    await user.click(await screen.findByTitle('common.edit'));
    await waitFor(() =>
      expect(screen.getByLabelText('system.roleIds')).toHaveValue('role-analyst-id'),
    );
    await user.clear(screen.getByLabelText('system.displayName'));
    await user.type(screen.getByLabelText('system.displayName'), 'Alice Updated');
    await user.click(screen.getByRole('button', { name: 'common.save' }));
    expect(mockPut).toHaveBeenCalledWith('/system/users/managed-user', {
      display_name: 'Alice Updated',
      role_ids: ['role-analyst-id'],
      status: 'active',
    });
  });

  it.each([
    { roles: [] },
    {
      roles: [
        { id: 'role-one', name: 'analyst', tenant_id: 'test-tenant' },
        { id: 'role-two', name: 'analyst', tenant_id: 'test-tenant' },
      ],
    },
    { roles: [{ id: 'role-other-tenant', name: 'analyst', tenant_id: 'other-tenant' }] },
  ])(
    'blocks saving when the returned role names cannot map uniquely within the tenant ($roles)',
    async ({ roles }) => {
      mockUsersAndRoles(roles);
      const user = userEvent.setup();
      renderWithProviders(<SystemSettingsPage />);
      await user.click(await screen.findByTitle('common.edit'));
      expect(await screen.findByRole('alert')).toHaveTextContent('system.roleMappingError');
      expect(screen.getByRole('button', { name: 'common.save' })).toBeDisabled();
      expect(mockPut).not.toHaveBeenCalled();
    },
  );

  it('blocks unknown role IDs but permits explicitly removing all assigned roles', async () => {
    mockUsersAndRoles([{ id: 'role-analyst-id', name: 'analyst', tenant_id: 'test-tenant' }]);
    mockPut.mockResolvedValue({ data: {} });
    const user = userEvent.setup();
    renderWithProviders(<SystemSettingsPage />);
    await user.click(await screen.findByTitle('common.edit'));
    const input = await screen.findByLabelText('system.roleIds');
    await waitFor(() => expect(input).toHaveValue('role-analyst-id'));
    await user.clear(input);
    await user.type(input, 'unknown-role');
    expect(await screen.findByRole('alert')).toHaveTextContent('system.roleIdsInvalid');
    expect(screen.getByRole('button', { name: 'common.save' })).toBeDisabled();
    await user.clear(input);
    await user.click(screen.getByRole('button', { name: 'common.save' }));
    expect(mockPut).toHaveBeenCalledWith('/system/users/managed-user', {
      display_name: 'Alice',
      role_ids: [],
      status: 'active',
    });
  });

  it('prevents saving while roles fail to load and can recover by retrying', async () => {
    mockGet.mockImplementation((url: string) =>
      url.endsWith('/roles')
        ? Promise.reject(new Error('Role lookup unavailable'))
        : Promise.resolve({ data: { data: [managedUser] } }),
    );
    const user = userEvent.setup();
    renderWithProviders(<SystemSettingsPage />);
    await user.click(await screen.findByTitle('common.edit'));
    expect(await screen.findByRole('alert')).toHaveTextContent('Role lookup unavailable');
    expect(screen.getByRole('button', { name: 'common.save' })).toBeDisabled();
    mockUsersAndRoles([{ id: 'role-analyst-id', name: 'analyst', tenant_id: 'test-tenant' }]);
    await user.click(screen.getByRole('button', { name: 'common.retry' }));
    await waitFor(() =>
      expect(screen.getByLabelText('system.roleIds')).toHaveValue('role-analyst-id'),
    );
    expect(screen.getByRole('button', { name: 'common.save' })).toBeEnabled();
    expect(mockPut).not.toHaveBeenCalled();
  });
});
