import type { ProductResource } from './resource-types';

export interface PendingResourceAction {
  record: ProductResource;
  name: string;
  payload: Record<string, unknown>;
}

export function isReadAction(kind: string, action: string) {
  if (kind === 'development_config' && action === 'validate') return true;
  return (
    (['metadata_lineage', 'asset_map'].includes(kind) && action === 'analyze') ||
    (['naming_standard', 'field_standard', 'enum_standard', 'unit_standard'].includes(kind) &&
      action === 'validate')
  );
}
export function isExecutionAction(kind: string, action: string) {
  return (
    (['atomic_metric', 'derived_metric', 'composite_metric'].includes(kind) &&
      action === 'calculate') ||
    ([
      'sql_document',
      'development_workflow',
      'bi_report',
      'data_api',
      'quality_task',
      'metadata_collection',
      'recognition_task',
    ].includes(kind) &&
      action === 'execute') ||
    (kind === 'development_workflow' &&
      ['register_schedule', 'unschedule', 'cancel'].includes(action)) ||
    (kind === 'recognition_task' && action === 'cancel') ||
    (kind === 'master_code_rule' && action === 'allocate')
  );
}

export function canUseResourceAction(
  kind: string,
  action: string,
  canRead: boolean,
  canWrite: boolean,
  permissions: readonly string[],
) {
  const extra = actionPermission(kind, action);
  return (
    canRead &&
    (canWrite || isReadAction(kind, action) || isExecutionAction(kind, action)) &&
    (!extra || permissions.includes(extra))
  );
}

export function actionParameters(
  record: ProductResource,
): { name: string; data_type: string; required: boolean; has_default?: boolean }[] {
  const definition = record.properties.definition;
  const source =
    record.kind === 'development_workflow' &&
    definition &&
    typeof definition === 'object' &&
    'parameters' in definition
      ? definition.parameters
      : record.kind === 'message_template'
        ? record.properties.variables
        : record.properties.parameters;
  if (!Array.isArray(source)) return [];
  return source.flatMap<{
    name: string;
    data_type: string;
    required: boolean;
    has_default?: boolean;
  }>((parameter) => {
    if (typeof parameter === 'string' && record.kind === 'message_template')
      return [{ name: parameter, data_type: 'VARCHAR', required: true }];
    if (
      parameter &&
      typeof parameter === 'object' &&
      typeof parameter.name === 'string' &&
      typeof parameter.data_type === 'string'
    )
      return [
        {
          name: parameter.name,
          data_type: parameter.data_type,
          required: parameter.required === true,
          has_default: Object.hasOwn(parameter, 'default_value'),
        },
      ];
    return [];
  });
}

export function initialActionPayload(
  record: ProductResource,
  action: string,
): Record<string, unknown> {
  const parameters: Record<string, unknown> = {};
  if (['execute', 'preview'].includes(action))
    for (const parameter of actionParameters(record))
      if (
        parameter.required &&
        !parameter.has_default &&
        parameter.data_type === 'BOOLEAN' &&
        !record.properties.configuration_id
      )
        parameters[parameter.name] = false;
  return {
    ...(record.kind === 'development_config' && ['publish', 'withdraw'].includes(action)
      ? { idempotency_key: crypto.randomUUID(), reason: '' }
      : {}),
    ...(record.kind === 'masking_rule' && action === 'preview' ? { sample: '' } : {}),
    ...(record.kind === 'user_group' && action === 'assign_members'
      ? {
          member_ids: Array.isArray(record.properties.member_ids)
            ? [...record.properties.member_ids]
            : [],
        }
      : {}),
    ...(record.kind === 'metadata_lineage' && action === 'analyze'
      ? {
          root_object_id: record.properties.target_object_id,
          direction: 'both',
          max_depth: 5,
          max_nodes: 100,
          max_edges: 200,
        }
      : {}),
    ...(record.kind === 'asset_map' && action === 'analyze'
      ? { max_nodes: 200, max_edges: 500 }
      : {}),
    ...(record.kind === 'development_workflow' &&
    ['register_schedule', 'unschedule'].includes(action)
      ? { idempotency_key: crypto.randomUUID(), reason: '' }
      : {}),
    ...(record.kind === 'tag' && ['assign', 'unassign'].includes(action)
      ? { idempotency_key: crypto.randomUUID(), target_kind: '', target_id: '' }
      : {}),
    ...(record.kind === 'alert_policy' && action === 'test_delivery'
      ? { idempotency_key: crypto.randomUUID() }
      : {}),
    ...([
      'send',
      'retry',
      'execute',
      'calculate',
      'evaluate',
      'allocate',
      'submit',
      'approve',
      'reject',
      'withdraw',
      'revoke',
    ].includes(action) && !(record.kind === 'asset_entry' && action === 'withdraw')
      ? { idempotency_key: crypto.randomUUID() }
      : {}),
    ...(Object.keys(parameters).length ? { parameters } : {}),
  };
}

export function actionAllowed(record: ProductResource, action: string) {
  const status = record.status;
  if (
    ['naming_standard', 'field_standard', 'enum_standard', 'unit_standard'].includes(record.kind)
  ) {
    if (action === 'revise') return status === 'published';
    if (action === 'publish') return status === 'draft';
  }
  if (record.kind === 'service_application') {
    if (['update', 'archive', 'issue_credential'].includes(action)) return status === 'active';
    if (action === 'delete') return false;
  }
  if (record.kind === 'service_subscription') {
    if (['submit', 'update'].includes(action)) return ['draft', 'rejected'].includes(status);
    if (['approve', 'reject'].includes(action)) return status === 'pending';
    if (action === 'revoke') return status === 'approved';
    if (action === 'delete') return false;
  }
  if (record.kind === 'asset_entry') {
    if (['register', 'update', 'delete'].includes(action))
      return ['draft', 'withdrawn'].includes(status);
    if (action === 'publish') return status === 'registered';
    if (action === 'withdraw') return ['registered', 'published'].includes(status);
  }
  if (record.kind === 'data_access_request') {
    if (action === 'submit' || action === 'update') return ['draft', 'rejected'].includes(status);
    if (action === 'withdraw') return ['draft', 'pending', 'approved'].includes(status);
    if (action === 'approve' || action === 'reject') return status === 'pending';
    if (action === 'revoke') return status === 'approved';
    if (action === 'delete') return false;
  }
  if (record.kind === 'data_model' && action === 'publish')
    return ['draft', 'publish_failed', 'publish_unknown', 'publishing'].includes(status);
  if (action === 'publish')
    return ['draft', 'inactive', 'publish_failed', 'publish_unknown'].includes(status);
  if (action === 'withdraw') return status === 'published';
  if (record.kind === 'master_data_record') {
    if (['approve', 'reject'].includes(action)) return status === 'pending';
    if (action === 'submit') return ['draft', 'rejected'].includes(status);
  }
  if (record.kind === 'system_message' && ['send', 'retry'].includes(action))
    return ['draft', 'failed'].includes(status);
  if (
    ['sql_document', 'development_workflow', 'bi_report', 'data_api', 'recognition_task'].includes(
      record.kind,
    ) &&
    action === 'execute'
  )
    return status === 'published';
  if (
    record.kind === 'development_workflow' &&
    ['register_schedule', 'unschedule'].includes(action)
  )
    return status === 'published';
  if (
    ['atomic_metric', 'derived_metric', 'composite_metric'].includes(record.kind) &&
    action === 'calculate'
  )
    return status === 'published';
  if (action === 'update')
    return (
      ['draft', 'inactive', 'publish_failed', 'rejected'].includes(status) ||
      (record.kind === 'master_data_record' && status === 'approved')
    );
  if (action === 'delete')
    return ['draft', 'inactive', 'publish_failed', 'rejected'].includes(status);
  return true;
}

export function actionPermission(kind: string, action: string): string | undefined {
  if (
    ['naming_standard', 'field_standard', 'enum_standard', 'unit_standard'].includes(kind) &&
    ['validate', 'publish', 'revise', 'withdraw'].includes(action)
  )
    return 'standards:execute';
  if (isExecutionAction(kind, action)) {
    if (['atomic_metric', 'derived_metric', 'composite_metric'].includes(kind))
      return 'metrics:execute';
    if (kind === 'recognition_task') return 'sensitive_data:execute';
    if (kind === 'quality_task') return 'quality:execute';
    if (kind === 'metadata_collection') return 'metadata:execute';
    if (kind === 'master_code_rule') return 'master_data:execute';
    if (kind === 'bi_report') return 'bi:execute';
    if (kind === 'data_api') return 'services:execute';
    return 'development:execute';
  }
  if (kind === 'master_data_record' && ['approve', 'reject'].includes(action))
    return 'master_data:approve';
  if (kind === 'service_subscription' && ['approve', 'reject', 'revoke'].includes(action))
    return 'services:approve';
  if (kind === 'asset_entry' && ['publish', 'withdraw'].includes(action)) return 'assets:execute';
  if (kind === 'data_api' && ['publish', 'execute'].includes(action)) return 'services:execute';
  if (kind === 'bi_report' && ['publish', 'execute'].includes(action)) return 'bi:execute';
  if (kind === 'data_model' && action === 'publish') return 'models:execute';
  return undefined;
}
