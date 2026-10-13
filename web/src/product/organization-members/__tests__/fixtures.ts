import { useAuthStore } from '../../../stores/auth';
import type { ProductResource } from '../../resource-types';
import type {
  OrganizationDecisionResult,
  OrganizationDetails,
  OrganizationMember,
  OrganizationUser,
} from '../types';

export const API = 'http://localhost:8080/api/v1';
export const administrator = {
  id: 'admin',
  username: 'admin',
  email: 'admin@example.invalid',
  tenant_id: 'tenant-org',
  tenant_name: 'Organization',
  roles: ['tenant_admin'],
  permissions: ['system:read', 'system:write', 'user:manage'],
};
export function login(
  permissions = administrator.permissions,
  token = 'organization-session',
  id = administrator.id,
) {
  const user = { ...administrator, id, permissions };
  localStorage.setItem('token', token);
  localStorage.setItem('user', JSON.stringify(user));
  useAuthStore.setState({ user, token, initialized: true, restoring: false, restoreError: null });
}
export const employee = (id: string): OrganizationUser => ({
  id,
  tenant_id: administrator.tenant_id,
  username: id === 'one' ? 'OwnerOne' : id === 'two' ? 'OwnerTwo' : 'FreeUser',
  display_name: null,
  status: 'active',
});
export function organization(id = 'a'): ProductResource {
  return {
    id,
    tenant_id: administrator.tenant_id,
    kind: 'organization',
    code: id,
    name: id === 'a' ? 'Org A' : 'Org B',
    description: null,
    parent_id: null,
    status: 'draft',
    properties: { sort_order: 0, manager_user_id: id === 'a' ? 'one' : null },
    version: id === 'a' ? 3 : 7,
    references: [],
    created_by: administrator.id,
    created_at: '2026-10-09T00:00:00Z',
    updated_at: '2026-10-09T00:00:00Z',
  };
}
export function member(id = 'one', inactive = false): OrganizationMember {
  return {
    user: { ...employee(id), status: inactive ? 'inactive' : 'active' },
    membership: {
      id: `membership-${id}`,
      tenant_id: administrator.tenant_id,
      user_id: id,
      organization_id: 'a',
      version: id === 'one' ? 1 : 5,
      changed_by: administrator.id,
      changed_at: '2026-10-09T00:00:00Z',
    },
    is_manager: id === 'one',
  };
}
export function details(id = 'a'): OrganizationDetails {
  return {
    organization: organization(id),
    manager: id === 'a' ? member() : null,
    manager_valid: id === 'a',
    member_count: id === 'a' ? 2 : 0,
    children_count: 0,
    legacy_membership_ignored: false,
  };
}
export function receipt(replayed = false): OrganizationDecisionResult {
  return {
    membership: { ...member().membership, organization_id: 'b', version: 2 },
    source: { id: 'a', code: 'a', name: 'Org A', version: 3, parent_id: null },
    target: { id: 'b', code: 'b', name: 'Org B', version: 7, parent_id: null },
    manager_changes: [],
    receipt_id: 'receipt-original-org',
    replayed,
    roles_unchanged: true,
  };
}
export function gate() {
  let release = () => {};
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release: () => release() };
}
