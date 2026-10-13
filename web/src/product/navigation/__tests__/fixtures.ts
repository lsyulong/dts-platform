import { useAuthStore } from '../../../stores/auth';
import type { ProductResource } from '../../resource-types';
import type { NavigationAssignment, NavigationDecisionResult, NavigationView } from '../types';

export const API = 'http://localhost:8080/api/v1';
export const administrator = {
  id: 'navigation-manager',
  username: 'manager',
  email: 'manager@example.invalid',
  tenant_id: 'tenant-navigation',
  tenant_name: 'Navigation',
  roles: ['tenant_admin'],
  permissions: ['system:read', 'system:write', 'user:manage'],
};

export function login(
  permissions = administrator.permissions,
  token = 'navigation-session',
  userId = administrator.id,
) {
  const user = { ...administrator, id: userId, permissions };
  localStorage.setItem('token', token);
  localStorage.setItem('user', JSON.stringify(user));
  useAuthStore.setState({ user, token, initialized: true, restoring: false, restoreError: null });
}

export const template: ProductResource = {
  id: 'menu-template-a',
  tenant_id: administrator.tenant_id,
  kind: 'menu_template',
  code: 'accounting',
  name: '财务导航模板',
  description: null,
  parent_id: null,
  status: 'draft',
  version: 7,
  properties: { menu_ids: ['menu-models'], validity_days: 30 },
  references: [{ field: 'menu_ids', target_kind: 'system_menu', target_id: 'menu-models' }],
  created_by: administrator.id,
  created_at: '2026-10-09T00:00:00Z',
  updated_at: '2026-10-09T00:00:00Z',
};

export function policy(overrides: Partial<NavigationView> = {}): NavigationView {
  return {
    tenant_id: administrator.tenant_id,
    mode: 'assigned',
    assignment_id: 'assignment-a',
    assignment_version: 4,
    template_id: template.id,
    template_version: template.version,
    expires_at: null,
    observed_at: new Date().toISOString(),
    filtered_count: 0,
    affects_navigation_only: true,
    nodes: [
      {
        id: 'menu-models',
        menu_version: 2,
        parent_id: null,
        name: '真实模型目录',
        path: '/warehouse/models',
        sort_order: 0,
        icon: null,
        source: 'template',
        recovery: false,
      },
    ],
    ...overrides,
  };
}

export function assignment(status: 'active' | 'revoked' = 'active'): NavigationAssignment {
  return {
    id: 'assignment-a',
    tenant_id: administrator.tenant_id,
    version: 5,
    status,
    template: {
      id: template.id,
      version: 7,
      name: template.name,
      edition: null,
      validity_days: 30,
      menus: [
        {
          id: 'menu-models',
          version: 2,
          parent_id: null,
          name: '真实模型目录',
          path: '/warehouse/models',
          additional_permission: null,
          visible: true,
          sort_order: 0,
          icon: null,
        },
      ],
    },
    assigned_by: administrator.id,
    assigned_at: '2026-10-09T00:00:00Z',
    expires_at: null,
    revoked_by: status === 'revoked' ? administrator.id : null,
    revoked_at: status === 'revoked' ? '2026-10-09T00:10:00Z' : null,
  };
}

export function receipt(
  replayed = false,
  status: 'active' | 'revoked' = 'active',
): NavigationDecisionResult {
  return { assignment: assignment(status), receipt_id: 'receipt-original-a', replayed };
}

export function gate() {
  let release = () => {};
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release: () => release() };
}
