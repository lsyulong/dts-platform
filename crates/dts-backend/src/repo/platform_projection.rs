use crate::BackendError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionAuthority {
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectionKind {
    pub kind: String,
    pub scope: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct ProjectionRequest {
    pub tenant_id: String,
    pub actor_id: String,
    pub expected_permissions: Vec<String>,
    pub kinds: Vec<ProjectionKind>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkSource {
    DataAccessApprovals,
    MasterDataApprovals,
    ServiceApprovals,
    QueuedJobs,
    ReconciliationJobs,
    UnreadMessages,
}
impl WorkSource {
    pub const ALL: [Self; 6] = [
        Self::DataAccessApprovals,
        Self::MasterDataApprovals,
        Self::ServiceApprovals,
        Self::QueuedJobs,
        Self::ReconciliationJobs,
        Self::UnreadMessages,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DataAccessApprovals => "data_access_approvals",
            Self::MasterDataApprovals => "master_data_approvals",
            Self::ServiceApprovals => "service_approvals",
            Self::QueuedJobs => "queued_jobs",
            Self::ReconciliationJobs => "reconciliation_jobs",
            Self::UnreadMessages => "unread_messages",
        }
    }
    pub fn permissions(self) -> &'static [&'static str] {
        match self {
            Self::DataAccessApprovals => &["data_security:read", "data_security:approve"],
            Self::MasterDataApprovals => &["master_data:read", "master_data:approve"],
            Self::ServiceApprovals => &["services:read", "services:approve"],
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkItem {
    pub id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: Option<i64>,
    pub title: String,
    pub status: String,
    pub created_at: String,
    pub due_at: Option<String>,
    pub route: Option<String>,
    pub required_permissions: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct WorkSourcePage {
    pub observed_at: DateTime<Utc>,
    pub data: Vec<WorkItem>,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct StatisticsRequest {
    pub actor: ProjectionRequest,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatisticsSource {
    Users,
    Datasources,
    SyncDefinitions,
    ProductDefinitions,
    SyncRuns,
    ProductJobs,
    LoginEvents,
    ResourceOperations,
    QueryAccess,
    ServiceCalls,
    Inbox,
}
impl StatisticsSource {
    pub const SYSTEM: [Self; 6] = [
        Self::Users,
        Self::Datasources,
        Self::SyncDefinitions,
        Self::ProductDefinitions,
        Self::SyncRuns,
        Self::ProductJobs,
    ];
    pub const USAGE: [Self; 5] = [
        Self::LoginEvents,
        Self::ResourceOperations,
        Self::QueryAccess,
        Self::ServiceCalls,
        Self::Inbox,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Users => "users",
            Self::Datasources => "datasources",
            Self::SyncDefinitions => "sync_definitions",
            Self::ProductDefinitions => "product_definitions",
            Self::SyncRuns => "sync_runs",
            Self::ProductJobs => "product_jobs",
            Self::LoginEvents => "login_events",
            Self::ResourceOperations => "resource_operations",
            Self::QueryAccess => "query_access",
            Self::ServiceCalls => "service_calls",
            Self::Inbox => "inbox",
        }
    }
    pub fn permissions(self) -> &'static [&'static str] {
        match self {
            Self::Users => &["user:manage"],
            Self::Datasources => &["datasource:read"],
            Self::SyncDefinitions | Self::SyncRuns => &["task:read"],
            Self::QueryAccess => &["data_security:read"],
            Self::ServiceCalls => &["services:read"],
            _ => &[],
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ProjectionCount {
    pub dimension: String,
    pub count: i64,
}
#[derive(Debug, Clone, Serialize)]
pub struct SourceStatistics {
    pub observed_at: DateTime<Utc>,
    pub data: Vec<ProjectionCount>,
    pub total: i64,
}

pub fn validate_request(p: &ProjectionRequest) -> Result<(), BackendError> {
    if p.tenant_id.is_empty()
        || p.actor_id.is_empty()
        || p.limit == 0
        || p.limit > 100
        || p.offset > 100_000
    {
        return Err(BackendError::ConstraintViolation(
            "投影请求或分页不合法".into(),
        ));
    }
    Ok(())
}
pub fn permitted(permissions: &[String], needs: &[&str]) -> bool {
    needs.iter().all(|p| permissions.iter().any(|v| v == p))
}
pub fn visible_kinds(p: &ProjectionRequest, execute: bool) -> Vec<String> {
    p.kinds
        .iter()
        .filter(|k| {
            permitted(&p.expected_permissions, &[&format!("{}:read", k.scope)])
                && (!execute
                    || permitted(&p.expected_permissions, &[&format!("{}:execute", k.scope)]))
        })
        .map(|k| k.kind.clone())
        .collect()
}

#[async_trait]
pub trait PlatformProjectionRepo: Send + Sync {
    async fn authority(
        &self,
        tenant_id: &str,
        actor_id: &str,
    ) -> Result<ProjectionAuthority, BackendError>;
    async fn work_source(
        &self,
        p: &ProjectionRequest,
        source: WorkSource,
    ) -> Result<WorkSourcePage, BackendError>;
    async fn statistics_source(
        &self,
        p: &StatisticsRequest,
        source: StatisticsSource,
    ) -> Result<SourceStatistics, BackendError>;
}
