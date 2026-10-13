import type { ResourceField } from './catalog-types';

export interface ResourceReference {
  field: string;
  target_kind: string;
  target_id: string;
}

export interface ProductResource {
  id: string;
  tenant_id: string;
  kind: string;
  code: string;
  name: string;
  description: string | null;
  parent_id: string | null;
  status: string;
  properties: Record<string, unknown>;
  references: ResourceReference[];
  version: number;
  created_by: string;
  created_at: string;
  updated_at: string;
  asset_state?: AssetState;
}

export interface AssetState {
  source_valid: boolean;
  stale: boolean;
  invalid_reason: string | null;
  current_source_version: number | null;
  publication: {
    asset_version: number;
    source_kind: string;
    source_id: string;
    source_version: number;
    published_by: string;
    published_at: string;
    publication_scope: string;
    physical_published: boolean;
  } | null;
}

export interface ResourceInput {
  code: string;
  name: string;
  description: string | null;
  parent_id: string | null;
  properties: Record<string, unknown>;
}

export interface ResourceSchema {
  kind: string;
  read_permission: string;
  write_permission: string;
  fields: ResourceField[];
  statuses: string[];
  actions: string[];
}

export interface ResourceAudit {
  id: string;
  resource_id: string;
  resource_kind: string;
  actor_id: string;
  operation: string;
  summary: Record<string, unknown>;
  created_at: string;
}

export interface ResourcePage {
  data: ProductResource[];
  total: number;
}

export interface ResourceFilters {
  q?: string;
  status?: string;
  limit: number;
  offset: number;
}

export interface ResourceActionResult {
  resource: ProductResource;
  result?: Record<string, unknown>;
  run_id?: string;
  next_path?: string;
  message?: string;
}
