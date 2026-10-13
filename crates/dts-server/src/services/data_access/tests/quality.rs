use super::query_service::fixture;
use super::*;
use crate::services::data_quality::{FrozenResource, QualitySnapshot, prepare_claimed};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use dts_backend::{
    BackendError,
    repo::{metadata_catalog::MetadataTable, product_job::*},
};
use serde_json::json;
use std::sync::{Arc, Mutex};

struct FixtureJobs(Mutex<ProductJob>);
fn unused<T>() -> Result<T, BackendError> {
    Err(BackendError::Configuration("此测试只读取质量任务".into()))
}
#[async_trait]
impl ProductJobRepo for FixtureJobs {
    async fn get(&self, lookup: &ProductJobLookup) -> Result<Option<ProductJob>, BackendError> {
        let job = self.0.lock().unwrap();
        Ok((lookup.tenant_id == job.tenant_id
            && lookup.job_id == job.id
            && lookup
                .actor_id
                .as_ref()
                .is_none_or(|actor| actor == &job.actor_id))
        .then(|| job.clone()))
    }
    async fn enqueue(
        &self,
        _: &EnqueueProductJobParams,
    ) -> Result<EnqueuedProductJob, BackendError> {
        unused()
    }
    async fn list(&self, filter: &ProductJobFilter) -> Result<ProductJobPage, BackendError> {
        let job = self.0.lock().unwrap();
        let visible = filter.tenant_id == job.tenant_id
            && filter
                .source_kind
                .as_ref()
                .is_none_or(|kind| kind == &job.source_kind)
            && filter
                .source_id
                .as_ref()
                .is_none_or(|id| id == &job.source_id);
        Ok(ProductJobPage {
            data: if visible && filter.offset == 0 {
                vec![job.clone()]
            } else {
                vec![]
            },
            total: i64::from(visible),
        })
    }
    async fn claim(&self, _: &ClaimProductJobParams) -> Result<Option<ProductJob>, BackendError> {
        unused()
    }
    async fn heartbeat(&self, _: &HeartbeatProductJobParams) -> Result<ProductJob, BackendError> {
        unused()
    }
    async fn start_node(
        &self,
        _: &StartProductJobNodeParams,
    ) -> Result<ProductJobNode, BackendError> {
        unused()
    }
    async fn finish_node(
        &self,
        _: &FinishProductJobNodeParams,
    ) -> Result<ProductJobNode, BackendError> {
        unused()
    }
    async fn finish(&self, _: &FinishProductJobParams) -> Result<ProductJob, BackendError> {
        unused()
    }
    async fn request_cancel(&self, _: &CancelProductJobParams) -> Result<ProductJob, BackendError> {
        unused()
    }
    async fn reconcile(&self, _: &ReconcileProductJobParams) -> Result<ProductJob, BackendError> {
        unused()
    }
}

fn running_job() -> (ProductJob, DataAccessTarget, String) {
    let mut target = target();
    let table = MetadataTable {
        table_catalog: target.binding.table_catalog.clone(),
        schema_name: target.binding.schema_name.clone(),
        table_name: target.binding.table_name.clone(),
        table_type: "BASE TABLE".into(),
        columns: target.columns.clone(),
    };
    target.binding.source_fingerprint = table.fingerprint().unwrap();
    let task = json!({"object_id":"object-1","rule_ids":["rule-1"]});
    let snapshot = QualitySnapshot {
        format_version: 1,
        executor: "quality_count_v1".into(),
        tenant_id: "tenant-a".into(),
        source_kind: "quality_task".into(),
        source_id: "task-1".into(),
        source_version: 1,
        properties: task.clone(),
        task: FrozenResource {
            id: "task-1".into(),
            version: 1,
            name: "Task".into(),
            properties: task,
        },
        object: FrozenResource {
            id: "object-1".into(),
            version: 1,
            name: "Object".into(),
            properties: json!({"metadata_object_id":"meta-1","columns":["amount"]}),
        },
        metadata: FrozenResource {
            id: "meta-1".into(),
            version: 1,
            name: "Orders".into(),
            properties: table.properties("tenant-a").unwrap(),
        },
        rules: vec![FrozenResource {
            id: "rule-1".into(),
            version: 1,
            name: "Amount present".into(),
            properties: json!({"object_id":"object-1","rule_type":"not_null","column_name":"amount","conditions":[]}),
        }],
    };
    let definition = serde_json::to_value(crate::services::data_quality::QualityNode {
        format_version: 1,
        node_type: "quality_check".into(),
        rule_index: 0,
        rule_id: "rule-1".into(),
        rule_version: 1,
        quality_object_id: "object-1".into(),
        metadata_object_id: "meta-1".into(),
        table_name: "warehouse.main.orders".into(),
        columns: vec![crate::services::data_quality::FrozenColumn {
            name: "amount".into(),
            data_type: "DOUBLE".into(),
            nullable: true,
        }],
        rule_type: "not_null".into(),
        column_name: "amount".into(),
        conditions: json!([]),
        sample_limit: None,
    })
    .unwrap();
    let now = Utc::now();
    let job = ProductJob {
        id: "job-1".into(),
        tenant_id: "tenant-a".into(),
        source_kind: "quality_task".into(),
        source_id: "task-1".into(),
        source_version: 1,
        actor_id: "user-a".into(),
        action: "execute_quality".into(),
        idempotency_key: "quality-1".into(),
        definition_snapshot: serde_json::to_value(snapshot).unwrap(),
        parameters: json!({}),
        status: ProductJobStatus::Running,
        cancel_requested: false,
        cancel_requested_by: None,
        can_retry: true,
        attempt: 1,
        lease_owner: Some("worker-1".into()),
        lease_fence: 1,
        lease_expires_at: Some(now + Duration::minutes(1)),
        result: None,
        error: None,
        reconciled_by: None,
        created_at: now,
        updated_at: now,
        started_at: Some(now),
        finished_at: None,
        nodes: vec![ProductJobNode {
            id: "rule_0".into(),
            name: "Amount present".into(),
            definition,
            depends_on: vec![],
            status: ProductJobNodeStatus::Running,
            attempt: 1,
            result: None,
            error: None,
            started_at: Some(now),
            finished_at: None,
        }],
    };
    let sql = prepare_claimed(&job).unwrap().remove(0).1;
    (job, target, sql)
}

#[tokio::test]
async fn full_table_grant_runs_exact_quality_compiler_without_free_sql_cte_bypass() {
    let (service, repo, metadata, actor) = fixture();
    let (job, target, sql) = running_job();
    {
        let mut grants = repo.grants.lock().unwrap();
        grants[0].target = target.clone();
        grants[0].columns = target
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
    }
    *metadata.binding.lock().unwrap() = Some(target.binding);
    let jobs = Arc::new(FixtureJobs(Mutex::new(job.clone())));
    let service = service.with_job_repo(jobs);
    assert!(service.authorize(&actor, &sql).await.is_err());
    let query = service
        .authorize_quality(&actor, &job, "rule_0", &sql)
        .await
        .unwrap();
    assert!(!query.evidence().all_data);
    assert_eq!(query.evidence().columns, ["amount", "id", "secret"]);
    assert!(service.reauthorize(&actor, &query).await.is_ok());
    assert!(
        service
            .authorize_quality(&actor, &job, "rule_0", &(sql + " "))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn quality_requires_real_job_repository_and_complete_current_grant() {
    let (service, repo, metadata, actor) = fixture();
    let (job, target, sql) = running_job();
    assert!(
        service
            .authorize_quality(&actor, &job, "rule_0", &sql)
            .await
            .is_err()
    );
    repo.grants.lock().unwrap()[0].target = target.clone();
    *metadata.binding.lock().unwrap() = Some(target.binding);
    let service = service.with_job_repo(Arc::new(FixtureJobs(Mutex::new(job.clone()))));
    assert!(
        service
            .authorize_quality(&actor, &job, "rule_0", &sql)
            .await
            .is_err()
    );
    repo.grants.lock().unwrap()[0].columns = vec!["id".into(), "amount".into(), "secret".into()];
    assert!(
        service
            .authorize_quality(&actor, &job, "other_node", &sql)
            .await
            .is_err()
    );
    *metadata.binding.lock().unwrap() = None;
    assert!(
        service
            .authorize_quality(&actor, &job, "rule_0", &sql)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn quality_reauthorization_checks_cancel_expiry_fence_and_metadata() {
    let (service, repo, metadata, actor) = fixture();
    let (job, target, sql) = running_job();
    {
        let mut grants = repo.grants.lock().unwrap();
        grants[0].target = target.clone();
        grants[0].columns = target
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
    }
    *metadata.binding.lock().unwrap() = Some(target.binding);
    let jobs = Arc::new(FixtureJobs(Mutex::new(job.clone())));
    let service = service.with_job_repo(jobs.clone());
    let query = service
        .authorize_quality(&actor, &job, "rule_0", &sql)
        .await
        .unwrap();
    jobs.0.lock().unwrap().cancel_requested = true;
    assert!(service.reauthorize(&actor, &query).await.is_err());
    jobs.0.lock().unwrap().cancel_requested = false;
    jobs.0.lock().unwrap().lease_fence = 2;
    assert!(service.reauthorize(&actor, &query).await.is_err());
    jobs.0.lock().unwrap().lease_fence = 1;
    jobs.0.lock().unwrap().lease_expires_at = Some(Utc::now() - Duration::seconds(1));
    assert!(service.reauthorize(&actor, &query).await.is_err());
    jobs.0.lock().unwrap().lease_expires_at = Some(Utc::now() + Duration::minutes(1));
    metadata
        .binding
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .object_version = 2;
    assert!(service.reauthorize(&actor, &query).await.is_err());
}

#[tokio::test]
async fn historical_quality_result_uses_current_reader_not_original_executor() {
    let (service, repo, metadata, actor) = fixture();
    let (mut job, target, sql) = running_job();
    job.actor_id = "original-executor".into();
    job.status = ProductJobStatus::Succeeded;
    job.lease_owner = None;
    job.lease_expires_at = None;
    job.nodes[0].status = ProductJobNodeStatus::Succeeded;
    job.nodes[0].result = Some(
        crate::services::data_quality::map_node_result(
            &job.nodes[0].definition,
            crate::services::query_svc::QueryResult {
                columns: vec![],
                rows: vec![serde_json::json!({"checked_rows":3,"failed_rows":1})],
                row_count: 1,
                suggested_chart: None,
            },
        )
        .unwrap(),
    );
    {
        let mut grants = repo.grants.lock().unwrap();
        grants[0].target = target.clone();
        grants[0].columns = target
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
    }
    *metadata.binding.lock().unwrap() = Some(target.binding);
    let jobs = Arc::new(FixtureJobs(Mutex::new(job.clone())));
    let service = service.with_job_repo(jobs);
    assert!(
        service
            .authorize_quality(&actor, &job, "rule_0", &sql)
            .await
            .is_err()
    );
    let query = service
        .authorize_quality_result(&actor, &job, "rule_0", &sql)
        .await
        .unwrap();
    assert!(service.reauthorize(&actor, &query).await.is_ok());
    repo.grants.lock().unwrap()[0].revoked_at = Some(Utc::now());
    assert!(service.reauthorize(&actor, &query).await.is_err());
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn quality_report_is_denied_without_guard_or_when_any_count_source_loses_access() {
    use crate::{
        api::query::QueryAccess,
        services::{data_quality::QualityService, query_svc::QueryResult},
    };
    let (guard, repo, metadata, actor) = fixture();
    let (mut job, target, _) = running_job();
    let now = Utc::now();
    job.status = ProductJobStatus::Succeeded;
    job.finished_at = Some(now);
    job.nodes[0].status = ProductJobNodeStatus::Succeeded;
    job.nodes[0].result = Some(
        crate::services::data_quality::map_node_result(
            &job.nodes[0].definition,
            QueryResult {
                columns: vec![],
                rows: vec![serde_json::json!({"checked_rows":3,"failed_rows":1})],
                row_count: 1,
                suggested_chart: None,
            },
        )
        .unwrap(),
    );
    {
        let mut grants = repo.grants.lock().unwrap();
        grants[0].target = target.clone();
        grants[0].columns = target
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
    }
    *metadata.binding.lock().unwrap() = Some(target.binding);
    let jobs = Arc::new(FixtureJobs(Mutex::new(job)));
    let resources = Arc::new(dts_backend_sqlite::SqliteProductResourceRepo::new(
        sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
    ));
    let quality = QualityService::new(
        resources,
        jobs.clone(),
        QueryAccess {
            tenant_id: Some("tenant-a".into()),
        },
    );
    let start = now - Duration::hours(1);
    let end = now + Duration::hours(1);
    assert!(quality.report(&actor, None, start, end).await.is_err());
    let quality = quality.with_query_authorization(Arc::new(guard.with_job_repo(jobs)));
    let report = quality.report(&actor, None, start, end).await.unwrap();
    assert_eq!(report.checked_rows, 3);
    assert_eq!(report.failed_rows, 1);
    repo.grants.lock().unwrap()[0].revoked_at = Some(Utc::now());
    assert!(quality.report(&actor, None, start, end).await.is_err());
}
