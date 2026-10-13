import type { ProductResource } from '../resource-types';
export type RevisionKind = 'sql_document' | 'development_workflow';
export interface RevisionSummary {
  id: string;
  tenant_id: string;
  source_kind: RevisionKind;
  source_id: string;
  source_version: number;
  snapshot_hash: string;
  saved_by: string;
  saved_at: string;
  reason: string;
}
export interface Revision extends RevisionSummary {
  snapshot: ProductResource;
}
export interface RevisionPage {
  data: RevisionSummary[];
  total: number;
}
export interface RevisionDiff {
  tenant_id: string;
  source_kind: RevisionKind;
  source_id: string;
  from_revision_id: string;
  to_revision_id: string | null;
  from_version: number;
  to_version: number;
  from_hash: string;
  to_hash: string;
  changes: { path: string; before: unknown; after: unknown }[];
  truncated: boolean;
  comparison_scope: 'editable_definition';
}
export interface SaveRevisionResult {
  revision: RevisionSummary;
  receipt_id: string;
  created: boolean;
  replayed: boolean;
}
export interface RestoreRevisionResult {
  resource: ProductResource;
  revision: RevisionSummary;
  receipt_id: string;
  replayed: boolean;
  requires_publish: boolean;
}
