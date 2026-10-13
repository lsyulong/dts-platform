use dts_backend::repo::platform_projection::{StatisticsSource, WorkSource};
pub(super) fn work(source: WorkSource) -> (&'static str, &'static str) {
    match source {
        WorkSource::DataAccessApprovals => (
            "FROM data_access_request a JOIN product_resource r ON r.tenant_id=a.tenant_id AND r.id=a.request_id AND r.kind='data_access_request' WHERE a.tenant_id=$1 AND a.requester_id<>$2 AND r.status='pending' AND $3::text IS NOT NULL",
            "r.id AS id,r.id AS source_id,r.kind AS source_kind,r.version AS source_version,r.name AS title,r.status,r.updated_at::text AS created_at,a.expires_at::text AS due_at",
        ),
        WorkSource::MasterDataApprovals => (
            "FROM master_record_identity a JOIN product_resource r ON r.tenant_id=a.tenant_id AND r.id=a.record_id AND r.kind='master_data_record' WHERE a.tenant_id=$1 AND a.submitted_by IS NOT NULL AND a.submitted_by<>$2 AND r.status='pending' AND $3::text IS NOT NULL",
            "r.id AS id,r.id AS source_id,r.kind AS source_kind,r.version AS source_version,r.name AS title,r.status,r.updated_at::text AS created_at,NULL::text AS due_at",
        ),
        WorkSource::ServiceApprovals => (
            "FROM service_subscription a JOIN product_resource r ON r.tenant_id=a.tenant_id AND r.id=a.subscription_id AND r.kind='service_subscription' WHERE a.tenant_id=$1 AND a.owner_id<>$2 AND a.requester_id<>$2 AND a.status='pending' AND r.status=a.status AND r.version=a.resource_version AND $3::text IS NOT NULL",
            "r.id AS id,r.id AS source_id,r.kind AS source_kind,r.version AS source_version,r.name AS title,r.status,r.updated_at::text AS created_at,a.expires_at::text AS due_at",
        ),
        WorkSource::QueuedJobs => (
            "FROM product_job j JOIN product_resource r ON r.tenant_id=j.tenant_id AND r.kind=j.source_kind AND r.id=j.source_id WHERE j.tenant_id=$1 AND j.actor_id=$2 AND j.status='queued' AND j.source_kind IN (SELECT jsonb_array_elements_text($3::jsonb))",
            "j.id AS id,j.source_id,j.source_kind,j.source_version,r.name AS title,j.status,j.created_at::text AS created_at,NULL::text AS due_at",
        ),
        WorkSource::ReconciliationJobs => (
            "FROM product_job j JOIN product_resource r ON r.tenant_id=j.tenant_id AND r.kind=j.source_kind AND r.id=j.source_id WHERE j.tenant_id=$1 AND j.actor_id=$2 AND j.status='reconciliation_required' AND j.source_kind IN (SELECT jsonb_array_elements_text($3::jsonb))",
            "j.id AS id,j.source_id,j.source_kind,j.source_version,r.name AS title,j.status,j.created_at::text AS created_at,NULL::text AS due_at",
        ),
        WorkSource::UnreadMessages => (
            "FROM message_delivery m WHERE m.tenant_id=$1 AND m.recipient_id=$2 AND m.read_at IS NULL AND $3::text IS NOT NULL",
            "m.id AS id,m.message_id AS source_id,m.source_kind,m.message_version AS source_version,m.subject AS title,'unread' AS status,m.delivered_at::text AS created_at,NULL::text AS due_at",
        ),
    }
}
pub(super) fn statistics(source: StatisticsSource) -> (&'static str, &'static str, &'static str) {
    match source {
        StatisticsSource::Users => (
            "FROM \"user\" r WHERE r.tenant_id=$1 AND r.status<>'deleted' AND $2::text IS NOT NULL AND $3::text IS NOT NULL",
            "r.status",
            "r.created_at",
        ),
        StatisticsSource::Datasources => (
            "FROM datasource r WHERE r.tenant_id=$1 AND $2::text IS NOT NULL AND $3::text IS NOT NULL",
            "r.health_status",
            "r.created_at",
        ),
        StatisticsSource::SyncDefinitions => (
            "FROM sync_task r WHERE r.tenant_id=$1 AND $2::text IS NOT NULL AND $3::text IS NOT NULL",
            "r.status",
            "r.created_at",
        ),
        StatisticsSource::ProductDefinitions => (
            "FROM product_resource r WHERE r.tenant_id=$1 AND $2::text IS NOT NULL AND r.kind IN (SELECT jsonb_array_elements_text($3::jsonb)) AND r.kind NOT IN ('login_event','operation_event','sensitive_finding','work_item','system_statistics','usage_statistics','quality_result','quality_report','api_call','asset_analysis','workflow_run','data_access_audit','data_access_approval')",
            "r.kind||':'||r.status",
            "r.created_at",
        ),
        StatisticsSource::SyncRuns => (
            "FROM task_run r JOIN sync_task s ON s.id=r.task_id WHERE s.tenant_id=$1 AND $2::text IS NOT NULL AND $3::text IS NOT NULL",
            "r.status",
            "r.start_time",
        ),
        StatisticsSource::ProductJobs => (
            "FROM product_job r WHERE r.tenant_id=$1 AND r.actor_id=$2 AND r.source_kind IN (SELECT jsonb_array_elements_text($3::jsonb))",
            "r.source_kind||':'||r.status",
            "r.created_at",
        ),
        StatisticsSource::LoginEvents => (
            "FROM product_resource r WHERE r.tenant_id=$1 AND r.kind='login_event' AND r.created_by=$2 AND $3::text IS NOT NULL",
            "r.status",
            "r.created_at",
        ),
        StatisticsSource::ResourceOperations => (
            "FROM product_resource_audit r WHERE r.tenant_id=$1 AND r.actor_id=$2 AND $3::text IS NOT NULL AND r.resource_kind<>'login_event'",
            "r.operation",
            "r.created_at",
        ),
        StatisticsSource::QueryAccess => (
            "FROM data_access_audit r WHERE r.tenant_id=$1 AND r.actor_id=$2 AND r.action='query' AND $3::text IS NOT NULL",
            "r.outcome",
            "r.created_at",
        ),
        StatisticsSource::ServiceCalls => (
            "FROM service_call r WHERE r.tenant_id=$1 AND r.owner_id=$2 AND $3::text IS NOT NULL",
            "r.outcome",
            "r.started_at",
        ),
        StatisticsSource::Inbox => (
            "FROM message_delivery r WHERE r.tenant_id=$1 AND r.recipient_id=$2 AND $3::text IS NOT NULL",
            "CASE WHEN r.read_at IS NULL THEN 'unread' ELSE 'read' END",
            "r.delivered_at",
        ),
    }
}
