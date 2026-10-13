import {
  productText as t,
  resourceField as f,
  referenceField as ref,
  type ProductModule,
  type ResourceField,
} from './catalog-types';
import { moduleDescriptions } from './catalog-copy';

export const crud = ['create', 'update', 'delete'];

const workspaceTypes: Record<string, ProductModule['workspaceType']> = {
  warehouse_category: 'tree',
  warehouse_layer: 'tree',
  warehouse_domain: 'tree',
  warehouse_process: 'tree',
  warehouse_mart: 'tree',
  warehouse_subject: 'tree',
  naming_standard: 'editor',
  field_standard: 'editor',
  enum_standard: 'editor',
  data_model: 'editor',
  atomic_metric: 'editor',
  derived_metric: 'editor',
  composite_metric: 'editor',
  sql_document: 'editor',
  development_workflow: 'workflow',
  realtime_task: 'workflow',
  workflow_run: 'monitor',
  development_config: 'editor',
  metadata_object: 'catalog',
  metadata_lineage: 'workflow',
  metadata_collection: 'workflow',
  master_data_model: 'editor',
  master_code_rule: 'editor',
  master_distribution: 'workflow',
  data_access_request: 'review',
  data_access_approval: 'review',
  data_access_audit: 'monitor',
  recognition_rule: 'editor',
  recognition_task: 'workflow',
  sensitive_finding: 'review',
  masking_rule: 'editor',
  watermark_trace: 'review',
  watermark_policy: 'editor',
  quality_monitor: 'monitor',
  quality_object: 'catalog',
  quality_rule: 'editor',
  quality_task: 'workflow',
  quality_result: 'monitor',
  quality_report: 'monitor',
  asset_map: 'workflow',
  asset_entry: 'catalog',
  asset_analysis: 'monitor',
  data_api: 'editor',
  service_subscription: 'review',
  api_call: 'monitor',
  bi_report: 'editor',
  alert_policy: 'editor',
};

export const catalogModule = (
  id: string,
  groupId: string,
  zh: string,
  en: string,
  path: string,
  reference: string[],
  resourceKind: string,
  fields: ResourceField[],
  description: string,
  actions: string[] = crud,
): ProductModule => ({
  id,
  groupId,
  title: t(zh, en),
  description: t(description, moduleDescriptions[id] ?? en),
  path,
  reference,
  resourceKind,
  implementation: 'new',
  fields,
  actions,
  workspaceType: workspaceTypes[resourceKind] ?? 'table',
});

export const existingModule = (
  id: string,
  groupId: string,
  zh: string,
  en: string,
  path: string,
  reference: string[],
  permission: string,
  description: string,
  existingPath = path,
): ProductModule => ({
  id,
  groupId,
  title: t(zh, en),
  description: t(description, moduleDescriptions[id] ?? en),
  path,
  reference,
  permission,
  existingPath,
  implementation: 'existing',
  workspaceType:
    existingPath === '/query'
      ? 'editor'
      : existingPath === '/datasources' || existingPath === '/tasks'
        ? 'table'
        : 'monitor',
});

export const owner = (required = false) => ref('owner_id', '负责人', 'Owner', 'user', required);

export const expression = () => f('expression', '表达式', 'Expression', 'textarea', true);

export const sql = () => f('sql', 'SQL 定义', 'SQL definition', 'textarea', true);
