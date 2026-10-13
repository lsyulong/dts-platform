import { lazy, Suspense, type ReactNode } from 'react';
import type { ProductModule } from './catalog-types';
import type { Tab } from '../pages/system/types';

const OpsCenterPage = lazy(() => import('../pages/ops'));
const MonitoringPage = lazy(() => import('../pages/monitoring'));
const SystemSettingsPage = lazy(() => import('../pages/system'));

const ProductModulePage = lazy(() => import('./ProductModulePage'));
const ProfilePage = lazy(() => import('../pages/product/ProfilePage'));
const TaskLogsPage = lazy(() => import('../pages/product/TaskLogsPage'));
const MessagesPage = lazy(() => import('../pages/product/MessagesPage'));
const NotificationConnectionsPage = lazy(
  () => import('../pages/product/NotificationConnectionsPage'),
);
const JobRunsPage = lazy(() => import('../pages/product/JobRunsPage'));
const DevelopmentOverviewPage = lazy(() => import('../pages/product/DevelopmentOverviewPage'));
const QualityReportPage = lazy(() => import('../pages/product/QualityReportPage'));
const SystemContentPage = lazy(() => import('../pages/product/SystemContentPage'));
const MetadataCollectionPage = lazy(() => import('../pages/product/MetadataCollectionPage'));
const DataAccessApprovalsPage = lazy(() => import('../pages/product/DataAccessApprovalsPage'));
const DataAccessAuditPage = lazy(() => import('../pages/product/DataAccessAuditPage'));
const MyDataAccessPage = lazy(() => import('../pages/product/MyDataAccessPage'));
const BiReportPage = lazy(() => import('../pages/product/BiReportPage'));
const DataServicesPage = lazy(() => import('../pages/product/DataServicesPage'));
const AssetUsagePage = lazy(() => import('../pages/product/AssetUsagePage'));
const ServiceApplicationsPage = lazy(() => import('../pages/product/ServiceApplicationsPage'));
const ServiceSubscriptionsPage = lazy(() => import('../pages/product/ServiceSubscriptionsPage'));
const ServiceCallsPage = lazy(() => import('../pages/product/ServiceCallsPage'));
const MetricsPage = lazy(() => import('../pages/product/MetricsPage'));
const SensitiveRecognitionPage = lazy(() => import('../pages/product/SensitiveRecognitionPage'));
const SensitiveFindingsPage = lazy(() => import('../pages/product/SensitiveFindingsPage'));
const MenuTemplatesPage = lazy(() => import('../pages/product/MenuTemplatesPage'));
const OrganizationsPage = lazy(() => import('../pages/product/OrganizationsPage'));
const WorkItemsPage = lazy(() => import('../pages/product/WorkItemsPage'));
const PlatformStatisticsPage = lazy(() => import('../pages/product/PlatformStatisticsPage'));
const QualityMonitorPage = lazy(() => import('../pages/product/QualityMonitorPage'));
const StandardsPage = lazy(() => import('../pages/product/StandardsPage'));
const DevelopmentRevisionsPage = lazy(() => import('../pages/product/DevelopmentRevisionsPage'));
const deferredPage = (page: ReactNode) => (
  <Suspense
    fallback={
      <div role="status" className="p-6 text-sm text-[var(--color-text-tertiary)]">
        …
      </div>
    }
  >
    {page}
  </Suspense>
);
const systemTabs: Record<string, Tab> = {
  users: 'users',
  roles: 'roles',
  tenants: 'tenants',
  permissions: 'permissions',
};

export default function ProductRoute({ module }: { module: ProductModule }): ReactNode {
  if (
    ['naming_standard', 'field_standard', 'enum_standard', 'unit_standard'].includes(
      module.resourceKind ?? '',
    )
  )
    return deferredPage(<StandardsPage module={module} />);
  if (['sql_document', 'development_workflow'].includes(module.resourceKind ?? ''))
    return deferredPage(<DevelopmentRevisionsPage module={module} />);
  if (module.resourceKind === 'quality_monitor')
    return deferredPage(<QualityMonitorPage module={module} />);
  if (module.resourceKind === 'organization')
    return deferredPage(<OrganizationsPage module={module} />);
  if (module.resourceKind === 'work_item') return deferredPage(<WorkItemsPage />);
  if (['system_statistics', 'usage_statistics'].includes(module.resourceKind ?? ''))
    return deferredPage(<PlatformStatisticsPage module={module} />);
  if (
    module.resourceKind &&
    [
      'atomic_metric',
      'derived_metric',
      'composite_metric',
      'metric_modifier',
      'metric_period',
    ].includes(module.resourceKind)
  )
    return deferredPage(<MetricsPage module={module} />);
  if (
    module.resourceKind &&
    ['data_classification', 'recognition_rule', 'recognition_task'].includes(module.resourceKind)
  )
    return deferredPage(<SensitiveRecognitionPage module={module} />);
  if (module.resourceKind === 'sensitive_finding')
    return deferredPage(<SensitiveFindingsPage module={module} />);
  if (module.resourceKind === 'menu_template')
    return deferredPage(<MenuTemplatesPage module={module} />);
  if (module.id === 'personal-center') return deferredPage(<ProfilePage />);
  if (module.id === 'integration-logs') return deferredPage(<TaskLogsPage />);
  if (module.id === 'messages') return deferredPage(<MessagesPage module={module} />);
  if (module.id === 'notification-connections')
    return deferredPage(<NotificationConnectionsPage />);
  if (module.id === 'workflow-operations') return deferredPage(<JobRunsPage />);
  if (module.id === 'development-overview') return deferredPage(<DevelopmentOverviewPage />);
  if (module.id === 'development-task-operations')
    return deferredPage(<JobRunsPage scope="development_tasks" />);
  if (module.id === 'quality-results') return deferredPage(<JobRunsPage scope="quality" />);
  if (module.id === 'quality-reports') return deferredPage(<QualityReportPage />);
  if (module.id === 'metadata-collection')
    return deferredPage(<MetadataCollectionPage module={module} />);
  if (module.id === 'data-access-approvals') return deferredPage(<DataAccessApprovalsPage />);
  if (module.id === 'data-access-audit') return deferredPage(<DataAccessAuditPage />);
  if (module.id === 'my-data-access') return deferredPage(<MyDataAccessPage module={module} />);
  if (module.id === 'personnel-bi') return deferredPage(<BiReportPage module={module} />);
  if (module.id === 'service-apis') return deferredPage(<DataServicesPage module={module} />);
  if (module.id === 'asset-analysis') return deferredPage(<AssetUsagePage />);
  if (module.id === 'service-applications')
    return deferredPage(<ServiceApplicationsPage module={module} />);
  if (module.id === 'service-catalog')
    return deferredPage(<ServiceSubscriptionsPage module={module} />);
  if (module.id === 'service-calls') return deferredPage(<ServiceCallsPage />);
  if (['announcements', 'portals'].includes(module.id))
    return deferredPage(<SystemContentPage module={module} />);
  if (systemTabs[module.id])
    return deferredPage(<SystemSettingsPage initialTab={systemTabs[module.id]} />);
  if (module.existingPath === '/ops') return deferredPage(<OpsCenterPage />);
  if (module.existingPath === '/monitoring') return deferredPage(<MonitoringPage />);
  return deferredPage(<ProductModulePage module={module} />);
}
