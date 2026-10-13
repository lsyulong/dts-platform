import type { ProductWorkspaceProps } from './workspace-types';
import ResourceTable from './ResourceTable';
import ResourceTreeWorkspace from './workspaces/ResourceTreeWorkspace';
import ReviewQueueWorkspace from './workspaces/ReviewQueueWorkspace';
import AssetCatalogWorkspace from './workspaces/AssetCatalogWorkspace';
import MonitoringWorkspace from './workspaces/MonitoringWorkspace';
import EditorWorkspace from './workspaces/EditorWorkspace';
import WorkflowWorkspace from './workspaces/WorkflowWorkspace';

export default function ProductWorkspace(props: ProductWorkspaceProps) {
  switch (props.module.workspaceType) {
    case 'tree':
      return <ResourceTreeWorkspace {...props} />;
    case 'review':
      return <ReviewQueueWorkspace {...props} />;
    case 'catalog':
      return <AssetCatalogWorkspace {...props} />;
    case 'monitor':
      return <MonitoringWorkspace {...props} />;
    case 'editor':
      return <EditorWorkspace {...props} />;
    case 'workflow':
      return <WorkflowWorkspace {...props} />;
    default:
      return <ResourceTable {...props} />;
  }
}
