import { useAuthStore } from '../../../stores/auth';
import type { ProductResource } from '../../resource-types';
import type { Revision, RevisionDiff, RevisionSummary } from '../types';
export const API = 'http://localhost:8080/api/v1';
export const actor = {
  id: 'admin',
  username: 'admin',
  email: 'admin@example.invalid',
  tenant_id: 'tenant-revisions',
  tenant_name: 'Revisions',
  roles: ['tenant_admin'],
  permissions: ['development:read', 'development:write'],
};
export function login(permissions = actor.permissions, token = 'revision-session', id = actor.id) {
  const user = { ...actor, id, permissions };
  localStorage.setItem('token', token);
  localStorage.setItem('user', JSON.stringify(user));
  useAuthStore.setState({ user, token, initialized: true, restoring: false, restoreError: null });
}
export function source(version = 5, status = 'draft'): ProductResource {
  return {
    id: 'document',
    tenant_id: actor.tenant_id,
    kind: 'sql_document',
    code: 'document',
    name: 'Actual SQL Document',
    description: null,
    parent_id: null,
    status,
    properties: { sql: 'SELECT 2 AS value', parameters: [] },
    references: [],
    version,
    created_by: actor.id,
    created_at: '2026-10-09T00:00:00Z',
    updated_at: '2026-10-09T00:00:00Z',
  };
}
export function summary(version = 3): RevisionSummary {
  return {
    id: `revision-${version}`,
    tenant_id: actor.tenant_id,
    source_kind: 'sql_document',
    source_id: 'document',
    source_version: version,
    snapshot_hash: 'a'.repeat(64),
    saved_by: 'original-author',
    saved_at: '2026-10-09T00:01:00Z',
    reason: `Actual revision ${version}`,
  };
}
export function revision(version = 3): Revision {
  return {
    ...summary(version),
    snapshot: { ...source(version), properties: { sql: 'SELECT 1 AS value', parameters: [] } },
  };
}
export function diff(): RevisionDiff {
  return {
    tenant_id: actor.tenant_id,
    source_kind: 'sql_document',
    source_id: 'document',
    from_revision_id: 'revision-3',
    to_revision_id: null,
    from_version: 3,
    to_version: 5,
    from_hash: 'a'.repeat(64),
    to_hash: 'b'.repeat(64),
    changes: [{ path: '/properties/sql', before: 'SELECT 1 AS value', after: 'SELECT 2 AS value' }],
    truncated: false,
    comparison_scope: 'editable_definition',
  };
}
export function gate() {
  let release = () => {};
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release: () => release() };
}
