import { http, HttpResponse } from 'msw';
import { server } from '../../../test/mocks/server';
import { useAuthStore } from '../../../stores/auth';
import type { ProductResource } from '../../../product/resource-types';
import type {
  StandardRevision,
  ModelStandardBindingSet,
  StandardInspectionResult,
} from '../standards-api';
export const API = 'http://localhost:8080/api/v1';
export const permissions = [
  'models:read',
  'models:write',
  'standards:read',
  'standards:write',
  'standards:execute',
  'metadata:read',
];
export function login(current = permissions, token = 'standard-session') {
  const user = {
    id: 'actor',
    tenant_id: 'tenant',
    tenant_name: 'Tenant',
    username: 'actor',
    email: 'actor@example.invalid',
    roles: ['standard_manager'],
    permissions: current,
  };
  localStorage.setItem('token', token);
  localStorage.setItem('user', JSON.stringify(user));
  useAuthStore.setState({ user, token, initialized: true, restoring: false, restoreError: null });
}
export function resource(
  kind: string,
  id: string,
  props: Record<string, unknown> = {},
): ProductResource {
  return {
    id,
    tenant_id: 'tenant',
    kind,
    code: id,
    name: id === 'standard' ? '实际字段标准' : id === 'metadata' ? '真实采集表' : '真实模型',
    description: null,
    parent_id: null,
    status: 'published',
    properties: props,
    references: [],
    version: 3,
    created_by: 'actor',
    created_at: '2026-10-09T10:00:00Z',
    updated_at: '2026-10-09T10:00:00Z',
  };
}
export const standard = resource('field_standard', 'standard', {
  data_type: 'BIGINT',
  nullable: false,
  definition: '业务主键',
});
export const release: StandardRevision = {
  tenant_id: 'tenant',
  kind: 'field_standard',
  id: 'standard',
  revision_version: 2,
  code: 'standard',
  name: '实际字段标准',
  properties: standard.properties,
  children: [],
  definition_hash: 'a'.repeat(64),
  state: 'active',
  state_version: 1,
  published_by: 'actor',
  published_at: '2026-10-09T10:00:00Z',
  withdrawn_by: null,
  withdrawn_at: null,
  withdrawal_reason: null,
};
export const model = resource('data_model', 'model', {
  layer_id: 'layer',
  table_name: 'main.orders',
  columns: [{ name: 'id', data_type: 'BIGINT', nullable: false }],
});
export const metadata = resource('metadata_object', 'metadata', {
  table_catalog: 'lake',
  schema_name: 'main',
  table_name: 'orders',
  columns: [{ name: 'id', data_type: 'BIGINT', nullable: false, ordinal_position: 1 }],
});
export const bindings: ModelStandardBindingSet = {
  tenant_id: 'tenant',
  model_id: 'model',
  model_version: 1,
  binding_version: 1,
  coverage: 'all_fields',
  columns: [
    {
      column_name: 'id',
      field_standard: {
        kind: 'field_standard',
        id: 'standard',
        revision_version: 2,
        definition_hash: 'a'.repeat(64),
      },
      naming_standard: null,
    },
  ],
  table_naming: null,
  model_properties_hash: 'b'.repeat(64),
  definition_hash: 'c'.repeat(64),
  created_by: 'actor',
  created_at: '2026-10-09T10:00:00Z',
};
export const inspection: StandardInspectionResult = {
  id: 'inspection',
  model_id: 'model',
  model_version: 3,
  binding_version: 1,
  metadata_id: 'metadata',
  metadata_version: 3,
  metadata_observed_at: '2026-10-09T10:00:00Z',
  source_fingerprint: 'actual-observed-fingerprint',
  evaluation_scope: 'collected_schema',
  state: 'conformant',
  issues: [],
  data_values_evaluated: false,
  unit_semantics_evaluated: false,
  dependency_versions: [bindings.columns[0].field_standard!],
  created_at: '2026-10-09T11:00:00Z',
};
export function handlers(binding: ModelStandardBindingSet | null = bindings) {
  server.use(
    http.get(`${API}/product/resources/:kind`, ({ params }) =>
      HttpResponse.json({
        data:
          params.kind === 'field_standard'
            ? [standard]
            : params.kind === 'metadata_object'
              ? [metadata]
              : [],
        total: params.kind === 'field_standard' || params.kind === 'metadata_object' ? 1 : 0,
      }),
    ),
    http.get(`${API}/product/standards/:kind/:id/revisions`, ({ params }) =>
      HttpResponse.json({
        data: params.kind === 'field_standard' && params.id === 'standard' ? [release] : [],
        total: params.kind === 'field_standard' && params.id === 'standard' ? 1 : 0,
      }),
    ),
    http.get(`${API}/product/standards/consumers/data_model/model/bindings`, () =>
      HttpResponse.json({ bindings: binding }),
    ),
  );
}
