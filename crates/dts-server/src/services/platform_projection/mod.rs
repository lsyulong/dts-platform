use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::{auth_svc::AuthService, product::catalog},
};
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::platform_projection::*};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationState {
    Known,
    Unknown,
    Restricted,
}
#[derive(Debug, Serialize)]
pub struct Observation {
    pub source: String,
    pub source_table: String,
    pub observed_at: DateTime<Utc>,
    pub state: ObservationState,
    pub total: Option<i64>,
    pub filter: String,
    pub error_code: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct WorkBucket {
    pub observation: Observation,
    pub data: Vec<WorkItem>,
}
#[derive(Debug, Serialize)]
pub struct WorkItemsProjection {
    pub tenant_id: String,
    pub actor_id: String,
    pub limit: u32,
    pub offset: u32,
    pub sources: Vec<WorkBucket>,
}
#[derive(Debug, Serialize)]
pub struct StatisticsBucket {
    pub observation: Observation,
    pub data: Vec<ProjectionCount>,
}
#[derive(Debug, Serialize)]
pub struct StatisticsProjection {
    pub tenant_id: String,
    pub actor_id: String,
    pub view: StatisticsView,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub sources: Vec<StatisticsBucket>,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatisticsView {
    System,
    Usage,
}
#[derive(Clone)]
pub struct PlatformProjectionService {
    repo: Arc<dyn PlatformProjectionRepo>,
    auth: AuthService,
}
impl PlatformProjectionService {
    pub fn new(repo: Arc<dyn PlatformProjectionRepo>, auth: AuthService) -> Self {
        Self { repo, auth }
    }
    async fn request(
        &self,
        actor: &AuthIdentity,
        limit: u32,
        offset: u32,
    ) -> Result<ProjectionRequest, AppError> {
        self.auth.validate_identity(actor).await?;
        let authority = self.repo.authority(&actor.tenant_id, &actor.sub).await?;
        let kinds = catalog::modules()?
            .iter()
            .filter_map(|m| {
                m.resource_kind.as_ref().map(|kind| ProjectionKind {
                    kind: kind.clone(),
                    scope: catalog::scope(m),
                    path: m.path.clone(),
                })
            })
            .collect();
        let p = ProjectionRequest {
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            expected_permissions: authority.permissions,
            kinds,
            limit,
            offset,
        };
        validate_request(&p).map_err(|_| {
            AppError::BadRequest("分页范围应为limit 1..100、offset 0..100000".into())
        })?;
        Ok(p)
    }
    async fn recheck(&self, actor: &AuthIdentity, p: &ProjectionRequest) -> Result<(), AppError> {
        self.auth.validate_identity(actor).await?;
        let current = self.repo.authority(&p.tenant_id, &p.actor_id).await?;
        if current.permissions != p.expected_permissions {
            return Err(AppError::Forbidden("读取期间权限发生变化，请刷新".into()));
        }
        Ok(())
    }
    pub async fn work_items(
        &self,
        actor: &AuthIdentity,
        source: Option<WorkSource>,
        limit: u32,
        offset: u32,
    ) -> Result<WorkItemsProjection, AppError> {
        let p = self.request(actor, limit, offset).await?;
        let selected = source.map_or_else(|| WorkSource::ALL.to_vec(), |s| vec![s]);
        let mut sources = Vec::new();
        for source in selected {
            let observed = work_provenance(source);
            let (mut observation, data) = match self.repo.work_source(&p, source).await {
                Ok(page) => (
                    known(
                        source.as_str(),
                        observed.0,
                        observed.1,
                        page.observed_at,
                        page.total,
                    ),
                    page.data,
                ),
                Err(e) => (
                    unavailable(source.as_str(), observed.0, observed.1, e),
                    Vec::new(),
                ),
            };
            let data = data
                .into_iter()
                .map(|item| route_item(item, source, &p))
                .collect();
            observation
                .filter
                .push_str("; sources are observed independently, no global snapshot");
            sources.push(WorkBucket { observation, data });
        }
        self.recheck(actor, &p).await?;
        Ok(WorkItemsProjection {
            tenant_id: p.tenant_id,
            actor_id: p.actor_id,
            limit,
            offset,
            sources,
        })
    }
    pub async fn statistics(
        &self,
        actor: &AuthIdentity,
        view: StatisticsView,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<StatisticsProjection, AppError> {
        if end <= start
            || end - start > chrono::Duration::days(366)
            || end > Utc::now() + chrono::Duration::minutes(5)
        {
            return Err(AppError::BadRequest(
                "UTC统计窗口应为[start,end)，最多366天且不能在未来".into(),
            ));
        }
        let p = self.request(actor, 1, 0).await?;
        if view == StatisticsView::System && !permitted(&p.expected_permissions, &["system:read"]) {
            return Err(AppError::Forbidden("系统统计需要system:read".into()));
        }
        let request = StatisticsRequest {
            actor: p,
            start,
            end,
        };
        let selected = if view == StatisticsView::System {
            StatisticsSource::SYSTEM.as_slice()
        } else {
            StatisticsSource::USAGE.as_slice()
        };
        let mut sources = Vec::new();
        for source in selected {
            let observed = statistics_provenance(*source);
            let (observation, data) = match self.repo.statistics_source(&request, *source).await {
                Ok(page) => (
                    known(
                        source.as_str(),
                        observed.0,
                        observed.1,
                        page.observed_at,
                        page.total,
                    ),
                    page.data,
                ),
                Err(e) => (
                    unavailable(source.as_str(), observed.0, observed.1, e),
                    Vec::new(),
                ),
            };
            sources.push(StatisticsBucket { observation, data });
        }
        self.recheck(actor, &request.actor).await?;
        Ok(StatisticsProjection {
            tenant_id: request.actor.tenant_id,
            actor_id: request.actor.actor_id,
            view,
            start,
            end,
            sources,
        })
    }
}
fn known(source: &str, table: &str, filter: &str, at: DateTime<Utc>, total: i64) -> Observation {
    Observation {
        source: source.into(),
        source_table: table.into(),
        observed_at: at,
        state: ObservationState::Known,
        total: Some(total),
        filter: filter.into(),
        error_code: None,
    }
}
fn unavailable(source: &str, table: &str, filter: &str, error: BackendError) -> Observation {
    let restricted = matches!(error, BackendError::Forbidden(_));
    Observation {
        source: source.into(),
        source_table: table.into(),
        observed_at: Utc::now(),
        state: if restricted {
            ObservationState::Restricted
        } else {
            ObservationState::Unknown
        },
        total: None,
        filter: filter.into(),
        error_code: Some(
            if restricted {
                "source_permission_denied"
            } else {
                "source_read_failed"
            }
            .into(),
        ),
    }
}
fn route_item(mut item: WorkItem, source: WorkSource, p: &ProjectionRequest) -> WorkItem {
    item.route = match source {
        WorkSource::DataAccessApprovals => Some("/security/approvals".into()),
        WorkSource::MasterDataApprovals => Some("/master-data/records".into()),
        WorkSource::ServiceApprovals => {
            item.route.or_else(|| Some("/services/applications".into()))
        },
        WorkSource::UnreadMessages => Some("/system/messages".into()),
        WorkSource::QueuedJobs | WorkSource::ReconciliationJobs => p
            .kinds
            .iter()
            .find(|k| k.kind == item.source_kind)
            .map(|k| format!("{}?job={}", k.path, item.id)),
    };
    item
}
fn work_provenance(s: WorkSource) -> (&'static str, &'static str) {
    match s {
        WorkSource::DataAccessApprovals => (
            "data_access_request + product_resource",
            "tenant=current; status=pending; requester!=actor; current read+approve",
        ),
        WorkSource::MasterDataApprovals => (
            "master_record_identity + product_resource",
            "tenant=current; status=pending; actual submitted_by!=actor; current read+approve",
        ),
        WorkSource::ServiceApprovals => (
            "service_subscription + product_resource",
            "tenant=current; typed/resource pending versions match; owner!=actor; requester!=actor; current read+approve",
        ),
        WorkSource::QueuedJobs => (
            "product_job + product_resource",
            "tenant=current; actor=current; status=queued; current source domain read+execute; awaiting worker, not manual completion",
        ),
        WorkSource::ReconciliationJobs => (
            "product_job + product_resource",
            "tenant=current; actor=current; status=reconciliation_required; current source domain read+execute; inspect real run before reconciliation",
        ),
        WorkSource::UnreadMessages => (
            "message_delivery",
            "tenant=current; recipient=current; read_at IS NULL; no message body in this projection",
        ),
    }
}
fn statistics_provenance(s: StatisticsSource) -> (&'static str, &'static str) {
    match s {
        StatisticsSource::Users => (
            "user",
            "tenant=current; status!=deleted; created_at in UTC window; current user:manage",
        ),
        StatisticsSource::Datasources => (
            "datasource",
            "tenant=current; created_at in UTC window; health_status is stored probe status, not live health; current datasource:read",
        ),
        StatisticsSource::SyncDefinitions => (
            "sync_task",
            "tenant=current; created_at in UTC window; current definition status; NOT executions; current task:read",
        ),
        StatisticsSource::ProductDefinitions => (
            "product_resource",
            "tenant=current; created_at in UTC window; current domain read; directory definitions only, exclude event/result projections; NOT executions",
        ),
        StatisticsSource::SyncRuns => (
            "task_run JOIN sync_task",
            "tenant=current via actual task; start_time in UTC window; persisted execution status; current task:read; not external-engine inventory",
        ),
        StatisticsSource::ProductJobs => (
            "product_job",
            "tenant=current; actor=current; created_at in UTC window; current source domain read; persisted job status including queued/reconciliation_required",
        ),
        StatisticsSource::LoginEvents => (
            "product_resource(kind=login_event)",
            "tenant=current; actual created_by=actor; created_at in UTC window; only persisted actor-linked login events; unassigned authentication failures excluded",
        ),
        StatisticsSource::ResourceOperations => (
            "product_resource_audit",
            "tenant=current; actor=current; created_at in UTC window; persisted resource writes; excludes login_event creation; not all platform operations",
        ),
        StatisticsSource::QueryAccess => (
            "data_access_audit(action=query)",
            "tenant=current; actor=current; created_at in UTC window; recorded query outcomes; not query engine row counts or elapsed time",
        ),
        StatisticsSource::ServiceCalls => (
            "service_call",
            "tenant=current; owner=current; started_at in UTC window; reserved calls grouped by actual outcome; pending reservation != succeeded; pre-reservation denials are not calls",
        ),
        StatisticsSource::Inbox => (
            "message_delivery",
            "tenant=current; recipient=current; delivered_at in UTC window; current read_at state",
        ),
    }
}

#[cfg(test)]
mod tests;
