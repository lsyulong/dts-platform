import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import client from '../api/client';
import { captureSession, isCurrentSession, type SessionIdentity } from '../lib/session';
import { useAuthStore } from '../stores/auth';
import { selectionSessionScope } from './business-editors/option-sources';
import type {
  ProductResource,
  ResourceAudit,
  ResourceFilters,
  ResourceInput,
  ResourcePage,
  ResourceSchema,
  ResourceActionResult,
} from './resource-types';

export interface WorkflowSchedulePlan {
  workflow_id: string;
  plan_version: number;
  source_version: number;
  cron_expression: string;
  timezone: string;
  retry_limit: number;
  authorized_by: string;
  last_operation: string;
  reason: string;
  scheduler_eid: string | null;
  last_error: string | null;
  changed_at: string;
}

export interface WorkflowScheduleStatus {
  scheduler_enabled: boolean;
  state: string;
  plan: WorkflowSchedulePlan | null;
}

export interface TagAssignment {
  id: string;
  tenant_id: string;
  tag_id: string;
  tag_version: number;
  target_kind: string;
  target_id: string;
  target_code: string;
  target_name: string;
  assigned_by: string;
  assigned_at: string;
}

export interface TagAssignmentPage {
  data: TagAssignment[];
  total: number;
}

export interface AlertPolicyDelivery {
  id: string;
  recipient_id: string;
  subject: string;
  body: string;
  channel: string;
  status: 'pending' | 'delivered' | 'failed';
  attempt_count: number;
  last_error: string | null;
  next_attempt_at: string | null;
  delivered_at: string | null;
  created_at: string;
}

export interface AlertPolicyEvaluation {
  id: string;
  policy_id: string;
  policy_version: number;
  event_type: string;
  window_start: string;
  window_end: string;
  observed_count: number;
  threshold: number;
  state: 'triggered' | 'within_threshold' | 'suppressed' | 'test';
  cooldown_until: string | null;
  actor_id: string;
  created_at: string;
  deliveries: AlertPolicyDelivery[];
  replayed: boolean;
}

export interface AlertPolicyEvaluationPage {
  data: AlertPolicyEvaluation[];
  total: number;
}

const resourcePath = (kind: string, id?: string) =>
  `/product/resources/${encodeURIComponent(kind)}${id ? `/${encodeURIComponent(id)}` : ''}`;

export function useWorkflowSchedule(workflowId: string | undefined, enabled = true) {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: [
      'workflow-schedule',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      workflowId,
    ],
    queryFn: ({ signal }) =>
      client
        .get<WorkflowScheduleStatus>(
          `/product/resources/development_workflow/${encodeURIComponent(workflowId!)}/schedule`,
          { signal, headers: { Authorization: `Bearer ${token}` } },
        )
        .then((response) => response.data),
    enabled: enabled && !!user && !!token && !!workflowId,
  });
}

export function useTagAssignments(tagId: string | undefined, enabled = true) {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: ['tag-assignments', user?.tenant_id, user?.id, selectionSessionScope(token), tagId],
    queryFn: ({ signal }) =>
      client
        .get<TagAssignmentPage>(
          `/product/resources/tag/${encodeURIComponent(tagId!)}/assignments`,
          {
            signal,
            params: { limit: 100, offset: 0 },
            headers: { Authorization: `Bearer ${token}` },
          },
        )
        .then((response) => response.data),
    enabled: enabled && !!user && !!token && !!tagId,
  });
}

export function useAlertPolicyEvaluations(policyId: string | undefined, enabled = true) {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: [
      'alert-policy-evaluations',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      policyId,
    ],
    queryFn: ({ signal }) =>
      client
        .get<AlertPolicyEvaluationPage>(
          `/product/resources/alert_policy/${encodeURIComponent(policyId!)}/evaluations`,
          {
            signal,
            params: { limit: 50, offset: 0 },
            headers: { Authorization: `Bearer ${token}` },
          },
        )
        .then((response) => response.data),
    enabled: enabled && !!user && !!token && !!policyId,
  });
}

export function useResourceSchemas() {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: [
      'product-schemas',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
    ],
    queryFn: ({ signal }) =>
      client
        .get<{ data: ResourceSchema[] }>('/product/schemas', {
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((r) => r.data.data),
    enabled: !!user && !!token,
    staleTime: 60_000,
  });
}

export function useProductResource(kind: string, id: string | undefined) {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: [
      'product-resource',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
      kind,
      id,
    ],
    enabled: !!user && !!token && !!id,
    queryFn: ({ signal }) =>
      client
        .get<ProductResource>(resourcePath(kind, id), {
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((response) => response.data),
    staleTime: 0,
  });
}

export function useProductResources(kind: string, filters: ResourceFilters, enabled = true) {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: [
      'product-resources',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
      kind,
      filters,
    ],
    queryFn: ({ signal }) =>
      client
        .get<ResourcePage>(resourcePath(kind), {
          params: filters,
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((r) => r.data),
    enabled: !!user && !!token && !!kind && enabled,
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[1] === user?.tenant_id &&
      previousQuery?.queryKey[2] === user?.id &&
      previousQuery?.queryKey[3] === selectionSessionScope(token) &&
      previousQuery?.queryKey[4] === JSON.stringify(user?.permissions ?? []) &&
      previousQuery?.queryKey[5] === kind
        ? previous
        : undefined,
  });
}

export function useResourceAudit(kind: string, id: string | undefined) {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: [
      'product-audit',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
      kind,
      id,
    ],
    queryFn: ({ signal }) =>
      client
        .get<{ data: ResourceAudit[] }>(`${resourcePath(kind, id)}/audit`, {
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((r) => r.data.data),
    enabled: !!user && !!token && !!id,
  });
}

export function useResourceMutation(kind: string) {
  const qc = useQueryClient();
  type Operation =
    | { type: 'create'; input: ResourceInput }
    | { type: 'update'; id: string; version: number; input: ResourceInput }
    | { type: 'delete'; id: string; version: number }
    | {
        type: 'action';
        id: string;
        version: number;
        action: string;
        payload?: Record<string, unknown>;
      };
  const mutation = useMutation({
    onMutate: ({ session }: { operation: Operation; session: SessionIdentity }) => session,
    mutationFn: async ({
      operation,
      session,
    }: {
      operation: Operation;
      session: SessionIdentity;
    }) => {
      if (!session.token) throw new Error('Invalid session');
      const headers = { Authorization: `Bearer ${session.token}` };
      switch (operation.type) {
        case 'create':
          return client
            .post<ProductResource>(resourcePath(kind), operation.input, { headers })
            .then((r) => r.data);
        case 'update':
          return client
            .put<ProductResource>(
              resourcePath(kind, operation.id),
              {
                ...operation.input,
                expected_version: operation.version,
              },
              { headers },
            )
            .then((r) => r.data);
        case 'delete':
          await client.delete(resourcePath(kind, operation.id), {
            params: { expected_version: operation.version },
            headers,
          });
          return null;
        case 'action':
          return client
            .post<ResourceActionResult>(
              `${resourcePath(kind, operation.id)}/actions/${encodeURIComponent(operation.action)}`,
              {
                expected_version: operation.version,
                payload: operation.payload ?? {},
              },
              { headers },
            )
            .then((r) => r.data);
      }
    },
    onSuccess: (_data, _operation, session) => {
      if (!isCurrentSession(session)) return;
      void qc.invalidateQueries({ queryKey: ['product-resources'] });
      void qc.invalidateQueries({ queryKey: ['product-resource'] });
      void qc.invalidateQueries({ queryKey: ['product-audit'] });
      void qc.invalidateQueries({ queryKey: ['workflow-schedule'] });
      void qc.invalidateQueries({ queryKey: ['tag-assignments'] });
      void qc.invalidateQueries({ queryKey: ['alert-policy-evaluations'] });
    },
  });
  return {
    isPending: mutation.isPending,
    mutateAsync: (operation: Operation) =>
      mutation.mutateAsync({ operation, session: captureSession() }),
  };
}
