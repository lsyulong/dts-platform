export interface UserItem {
  id: string;
  username: string;
  email: string;
  display_name?: string;
  tenant_id: string;
  tenant_name: string;
  roles: string[];
  status: string;
  created_at: string;
}

export interface RoleItem {
  id: string;
  name: string;
  tenant_id: string;
  description: string;
  is_system: boolean;
  permissions: string[];
  created_at: string;
}

export interface TenantItem {
  id: string;
  name: string;
  slug: string;
  status: string;
  created_at: string;
}

export interface PermissionItem {
  code: string;
  group: string;
}

export type Tab = 'users' | 'tenants' | 'roles' | 'permissions';

export const PERMISSION_GROUPS: Record<string, string> = {
  task: 'Sync Task',
  datasource: 'Data Source',
  alert: 'Alert',
  monitor: 'Monitor',
  ops: 'Ops',
  user: 'User',
  tenant: 'Tenant',
  audit: 'Audit',
};
