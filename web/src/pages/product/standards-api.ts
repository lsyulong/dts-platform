import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import client from '../../api/client';
import { useAuthStore } from '../../stores/auth';
import { captureSession, isCurrentSession } from '../../lib/session';
import { selectionSessionScope } from '../../product/business-editors/option-sources';
import type { ProductResource } from '../../product/resource-types';

export const standardKinds = [
  'naming_standard',
  'field_standard',
  'enum_standard',
  'unit_standard',
] as const;
export type StandardKind = (typeof standardKinds)[number];
export interface FrozenStandardRef {
  kind: StandardKind;
  id: string;
  revision_version: number;
  definition_hash: string;
}
export interface StandardRevision {
  tenant_id: string;
  kind: StandardKind;
  id: string;
  revision_version: number;
  code: string;
  name: string;
  properties: Record<string, unknown>;
  children: FrozenStandardRef[];
  definition_hash: string;
  state: 'active' | 'withdrawn';
  state_version: number;
  published_by: string;
  published_at: string;
  withdrawn_by: string | null;
  withdrawn_at: string | null;
  withdrawal_reason: string | null;
}
export interface StandardRevisionPage {
  data: StandardRevision[];
  total: number;
}
export interface StandardVersionRequest {
  id: string;
  version: number;
}
export interface ColumnStandardBinding {
  column_name: string;
  field_standard: FrozenStandardRef | null;
  naming_standard: FrozenStandardRef | null;
}
export interface ModelStandardBindingSet {
  tenant_id: string;
  model_id: string;
  model_version: number;
  binding_version: number;
  coverage: 'bound_fields' | 'all_fields';
  columns: ColumnStandardBinding[];
  table_naming: FrozenStandardRef | null;
  model_properties_hash: string;
  definition_hash: string;
  created_by: string;
  created_at: string;
}
export interface ModelStandardsRequest {
  expected_model_version: number;
  expected_binding_version: number | null;
  coverage: 'bound_fields' | 'all_fields';
  columns: {
    column_name: string;
    field_standard: StandardVersionRequest | null;
    naming_standard: StandardVersionRequest | null;
  }[];
  table_naming: StandardVersionRequest | null;
  upgrade: boolean;
}
export interface StandardInspectionResult {
  id: string;
  model_id: string;
  model_version: number;
  binding_version: number;
  metadata_id: string;
  metadata_version: number;
  metadata_observed_at: string;
  source_fingerprint: string;
  evaluation_scope: 'collected_schema';
  state: 'conformant' | 'non_conformant';
  issues: {
    column_name: string | null;
    code: string;
    message: string;
    expected: unknown;
    actual: unknown;
  }[];
  data_values_evaluated: false;
  unit_semantics_evaluated: false;
  dependency_versions: FrozenStandardRef[];
  created_at: string;
}
function record(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}
function positive(v: unknown): v is number {
  return typeof v === 'number' && Number.isSafeInteger(v) && v > 0;
}
function hash(v: unknown): v is string {
  return typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
}
function ref(v: unknown): v is FrozenStandardRef {
  return (
    record(v) &&
    standardKinds.some((k) => k === v.kind) &&
    typeof v.id === 'string' &&
    positive(v.revision_version) &&
    hash(v.definition_hash)
  );
}
function revision(
  v: unknown,
  tenant: string,
  kind: StandardKind,
  id: string,
): v is StandardRevision {
  return (
    record(v) &&
    v.tenant_id === tenant &&
    v.kind === kind &&
    v.id === id &&
    positive(v.revision_version) &&
    positive(v.state_version) &&
    hash(v.definition_hash) &&
    (v.state === 'active' || v.state === 'withdrawn') &&
    typeof v.name === 'string' &&
    typeof v.code === 'string' &&
    record(v.properties) &&
    Array.isArray(v.children) &&
    v.children.every(ref) &&
    typeof v.published_at === 'string' &&
    Number.isFinite(Date.parse(v.published_at))
  );
}
function binding(v: unknown, tenant: string, model: string): v is ModelStandardBindingSet {
  return (
    record(v) &&
    v.tenant_id === tenant &&
    v.model_id === model &&
    positive(v.binding_version) &&
    positive(v.model_version) &&
    hash(v.definition_hash) &&
    hash(v.model_properties_hash) &&
    ['bound_fields', 'all_fields'].includes(String(v.coverage)) &&
    (v.table_naming === null ||
      (ref(v.table_naming) && v.table_naming.kind === 'naming_standard')) &&
    Array.isArray(v.columns) &&
    v.columns.every(
      (c) =>
        record(c) &&
        typeof c.column_name === 'string' &&
        (c.field_standard === null ||
          (ref(c.field_standard) && c.field_standard.kind === 'field_standard')) &&
        (c.naming_standard === null ||
          (ref(c.naming_standard) && c.naming_standard.kind === 'naming_standard')),
    )
  );
}
export function standardIdentity() {
  const s = useAuthStore.getState();
  return JSON.stringify([s.user?.tenant_id, s.user?.id, s.token, s.user?.permissions]);
}
function useScope() {
  const user = useAuthStore((s) => s.user),
    token = useAuthStore((s) => s.token);
  return {
    user,
    token,
    key: [
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
    ],
  };
}
export function useStandardRevisions(kind: StandardKind, id: string | undefined, offset = 0) {
  const s = useScope();
  return useQuery({
    queryKey: ['standard-revisions', ...s.key, kind, id, offset],
    retry: false,
    enabled: !!s.user && !!s.token && !!id && s.user.permissions.includes('standards:read'),
    queryFn: async ({ signal }) => {
      const session = captureSession(),
        identity = standardIdentity();
      const reply = await client.get<unknown>(
        `/product/standards/${kind}/${encodeURIComponent(id ?? '')}/revisions`,
        { params: { limit: 20, offset }, signal, headers: { Authorization: `Bearer ${s.token}` } },
      );
      if (!isCurrentSession(session) || identity !== standardIdentity())
        throw new Error('权限或会话已变化 / Session or permissions changed');
      const v = reply.data;
      if (
        !s.user ||
        !id ||
        !record(v) ||
        !Array.isArray(v.data) ||
        !v.data.every((r) => revision(r, s.user!.tenant_id, kind, id)) ||
        typeof v.total !== 'number' ||
        !Number.isSafeInteger(v.total) ||
        v.total < 0 ||
        v.data.length > 20 ||
        v.data.length > v.total
      )
        throw new Error('标准版本返回格式无效 / Invalid standard revision response');
      return { data: v.data, total: v.total };
    },
  });
}
export function useModelStandardBindings(id: string, enabled = true) {
  const s = useScope();
  return useQuery({
    queryKey: ['model-standard-bindings', ...s.key, id],
    retry: false,
    enabled:
      enabled &&
      !!s.user &&
      !!s.token &&
      !!id &&
      s.user.permissions.includes('models:read') &&
      s.user.permissions.includes('standards:read'),
    queryFn: async ({ signal }) => {
      const session = captureSession(),
        identity = standardIdentity();
      const reply = await client.get<unknown>(
        `/product/standards/consumers/data_model/${encodeURIComponent(id)}/bindings`,
        { signal, headers: { Authorization: `Bearer ${s.token}` } },
      );
      if (!isCurrentSession(session) || identity !== standardIdentity())
        throw new Error('权限或会话已变化 / Session or permissions changed');
      const v = reply.data;
      if (
        !s.user ||
        !record(v) ||
        (v.bindings !== null && !binding(v.bindings, s.user.tenant_id, id))
      )
        throw new Error('模型绑定返回格式无效 / Invalid model binding response');
      return v.bindings;
    },
  });
}
type Operation =
  | {
      type: 'standard';
      kind: StandardKind;
      id: string;
      version: number;
      action: 'publish' | 'revise' | 'withdraw';
      payload?: Record<string, unknown>;
    }
  | { type: 'bind'; modelId: string; request: ModelStandardsRequest }
  | {
      type: 'inspect';
      modelId: string;
      request: {
        expected_model_version: number;
        expected_binding_version: number;
        metadata_id: string;
        metadata_version: number;
      };
    };
export function useStandardOperation() {
  const qc = useQueryClient();
  const mutation = useMutation({
    mutationFn: async ({
      operation,
      identity,
      session,
    }: {
      operation: Operation;
      identity: string;
      session: ReturnType<typeof captureSession>;
    }) => {
      const headers = { Authorization: `Bearer ${session.token}` };
      const reply =
        operation.type === 'standard'
          ? await client.post<unknown>(
              `/product/resources/${operation.kind}/${encodeURIComponent(operation.id)}/actions/${operation.action}`,
              { expected_version: operation.version, payload: operation.payload ?? {} },
              { headers },
            )
          : operation.type === 'bind'
            ? await client.put<unknown>(
                `/product/standards/consumers/data_model/${encodeURIComponent(operation.modelId)}/bindings`,
                operation.request,
                { headers },
              )
            : await client.post<unknown>(
                `/product/standards/consumers/data_model/${encodeURIComponent(operation.modelId)}/inspect`,
                operation.request,
                { headers },
              );
      if (!isCurrentSession(session) || identity !== standardIdentity())
        throw new Error('权限或会话已变化 / Session or permissions changed');
      const data = reply.data;
      if (operation.type === 'inspect') {
        if (
          !record(data) ||
          data.model_id !== operation.modelId ||
          data.model_version !== operation.request.expected_model_version ||
          data.binding_version !== operation.request.expected_binding_version ||
          data.metadata_id !== operation.request.metadata_id ||
          data.metadata_version !== operation.request.metadata_version ||
          data.evaluation_scope !== 'collected_schema' ||
          data.data_values_evaluated !== false ||
          data.unit_semantics_evaluated !== false ||
          !['conformant', 'non_conformant'].includes(String(data.state)) ||
          !Array.isArray(data.issues) ||
          data.issues.some(
            (issue) =>
              !record(issue) ||
              typeof issue.message !== 'string' ||
              typeof issue.code !== 'string' ||
              (issue.column_name !== null && typeof issue.column_name !== 'string'),
          ) ||
          (data.state === 'conformant') !== (data.issues.length === 0) ||
          typeof data.source_fingerprint !== 'string' ||
          typeof data.metadata_observed_at !== 'string' ||
          !Number.isFinite(Date.parse(data.metadata_observed_at)) ||
          !Array.isArray(data.dependency_versions) ||
          !data.dependency_versions.every(ref)
        )
          throw new Error('结构检查回执无效 / Invalid inspection receipt');
        return { type: 'inspect' as const, result: data as unknown as StandardInspectionResult };
      }
      if (
        !record(data) ||
        !record(data.resource) ||
        data.resource.tenant_id !== session.tenantId ||
        !positive(data.resource.version)
      )
        throw new Error('标准操作回执无效 / Invalid standard operation receipt');
      if (
        operation.type === 'bind' &&
        (data.resource.id !== operation.modelId ||
          data.resource.kind !== 'data_model' ||
          data.resource.version <= operation.request.expected_model_version ||
          !binding(data.bindings, session.tenantId ?? '', operation.modelId))
      )
        throw new Error('绑定操作回执无效 / Invalid binding receipt');
      if (
        operation.type === 'standard' &&
        (data.resource.id !== operation.id || data.resource.kind !== operation.kind)
      )
        throw new Error('版本操作回执无效 / Invalid revision receipt');
      if (operation.type === 'standard') {
        if (!revision(data.revision, session.tenantId ?? '', operation.kind, operation.id))
          throw new Error('版本证据回执无效 / Invalid revision evidence');
        if (
          operation.action === 'publish' &&
          (data.resource.status !== 'published' ||
            data.resource.version <= operation.version ||
            data.revision.state !== 'active' ||
            data.revision.revision_version !== data.resource.version)
        )
          throw new Error('发布未得到实际有效修订 / No active revision was published');
        if (
          operation.action === 'revise' &&
          (data.resource.status !== 'draft' ||
            data.resource.version <= operation.version ||
            data.revision.state !== 'active')
        )
          throw new Error('未创建实际下一版草稿 / No new draft was created');
        if (
          operation.action === 'withdraw' &&
          (data.revision.state !== 'withdrawn' ||
            data.revision.revision_version !== operation.payload?.revision_version ||
            data.revision.state_version !== Number(operation.payload?.expected_state_version) + 1)
        )
          throw new Error('未撤回实际指定修订 / The specified revision was not withdrawn');
      }
      return { type: operation.type, resource: data.resource as unknown as ProductResource };
    },
    onSuccess: async () => {
      await Promise.all(
        ['product-resources', 'standard-revisions', 'model-standard-bindings', 'product-audit'].map(
          (key) => qc.invalidateQueries({ queryKey: [key] }),
        ),
      );
    },
  });
  return {
    isPending: mutation.isPending,
    mutateAsync: (operation: Operation) =>
      mutation.mutateAsync({ operation, identity: standardIdentity(), session: captureSession() }),
  };
}
