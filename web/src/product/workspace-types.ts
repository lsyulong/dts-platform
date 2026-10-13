import type { ProductModule } from './catalog-types';
import type { ProductResource, ResourceSchema } from './resource-types';

export interface ProductWorkspaceProps {
  module: ProductModule;
  schema: ResourceSchema;
  records: ProductResource[];
  canWrite: boolean;
  busy?: boolean;
  status?: string;
  onStatusChange: (status: string) => void;
  onView: (record: ProductResource) => void;
  onEdit: (record: ProductResource) => void;
  onDelete: (record: ProductResource) => void;
  onAction: (record: ProductResource, action: string, payload?: Record<string, unknown>) => void;
}
