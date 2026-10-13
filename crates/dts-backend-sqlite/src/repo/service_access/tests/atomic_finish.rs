use super::{fixture::Fixture, *};
use crate::SqliteDataAccessRepo;
use dts_backend::ProductResourceRepo;
use dts_backend::repo::{
    data_access::*,
    metadata_catalog::{MetadataColumn, MetadataEngineBinding, MetadataTable},
};
use serde_json::json;

async fn ready() -> (Fixture, ServiceCallRecord, DataAccessGrant) {
    let f = Fixture::new(10).await;
    let table = MetadataTable {
        table_catalog: "lake".into(),
        schema_name: "main".into(),
        table_name: "orders".into(),
        table_type: "BASE TABLE".into(),
        columns: vec![MetadataColumn {
            name: "amount".into(),
            data_type: "BIGINT".into(),
            nullable: false,
            ordinal_position: 1,
        }],
    };
    let target = DataAccessTarget {
        metadata_id: "metadata".into(),
        binding: MetadataEngineBinding {
            engine: "swanlake".into(),
            tenant_id: "tenant".into(),
            object_version: 1,
            table_catalog: table.table_catalog.clone(),
            schema_name: table.schema_name.clone(),
            table_name: table.table_name.clone(),
            source_fingerprint: table.fingerprint().unwrap(),
        },
        columns: table.columns.clone(),
    };
    crate::SqliteProductResourceRepo::new(f.pool.clone())
        .create(&dts_backend::CreateResourceParams {
            id: "metadata".into(),
            tenant_id: "tenant".into(),
            kind: "metadata_object".into(),
            actor_id: "owner".into(),
            fields: dts_backend::ResourceWrite {
                code: "metadata".into(),
                name: "Metadata".into(),
                description: None,
                parent_id: None,
                status: "published".into(),
                properties: table.properties("tenant").unwrap(),
                references: vec![],
            },
        })
        .await
        .unwrap();
    sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at)
        VALUES('tenant','swanlake','lake','main','orders','metadata',1,'BASE TABLE',?1,?2,'present','fixture',CURRENT_TIMESTAMP)")
        .bind(json!(table.columns).to_string()).bind(&target.binding.source_fingerprint).execute(&f.pool).await.unwrap();
    let metadata = json!({"id":target.metadata_id,"version":1,"binding":target.binding,"columns":target.columns});
    sqlx::query("UPDATE product_resource SET properties=json_set(properties,'$.publication.metadata',json(?1)) WHERE id='api'")
        .bind(metadata.to_string()).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO role_permission(role_id,permission) VALUES('approve-role','data_security:approve')")
        .execute(&f.pool).await.unwrap();
    let access = SqliteDataAccessRepo::new(f.pool.clone());
    let request = access
        .save_request(&SaveDataAccessRequestParams {
            id: "request".into(),
            tenant_id: "tenant".into(),
            actor_id: "owner".into(),
            expected_version: None,
            code: "request".into(),
            name: "Request".into(),
            description: None,
            target,
            requested_columns: vec!["amount".into()],
            purpose: "Actual access".into(),
            expires_at: chrono::Utc::now() + chrono::Duration::days(2),
        })
        .await
        .unwrap();
    let decision = |version, actor: &str, action, key: &str| DecideDataAccessParams {
        tenant_id: "tenant".into(),
        request_id: "request".into(),
        expected_version: version,
        actor_id: actor.into(),
        decision: action,
        reason: "Reviewed".into(),
        expires_at: None,
        idempotency_key: key.into(),
    };
    let pending = access
        .decide(&decision(
            request.version,
            "owner",
            DataAccessDecision::Submit,
            "submit-data",
        ))
        .await
        .unwrap();
    let grant = access
        .decide(&decision(
            pending.request.version,
            "approver",
            DataAccessDecision::Approve,
            "approve-data",
        ))
        .await
        .unwrap()
        .grant
        .unwrap();
    f.application("app", "App").await;
    f.approve("app").await;
    let credential = f.credential("app", "credential").await;
    let call = f
        .repo
        .reserve_call(&f.reserve(&credential, "atomic"))
        .await
        .unwrap();
    (f, call, grant)
}
fn success(call: &ServiceCallRecord, grant: &DataAccessGrant) -> FinishServiceCallParams {
    FinishServiceCallParams {
        call: call.clone(),
        outcome: ServiceCallOutcome::Succeeded,
        duration_ms: 10,
        row_count: Some(1),
        access_audit: Some(RecordDataAccessParams {
            id: format!("service_call:{}", call.id),
            tenant_id: call.tenant_id.clone(),
            actor_id: call.owner_id.clone(),
            outcome: AccessOutcome::Succeeded,
            evidence: DataAccessEvidence {
                query_hash: "a".repeat(64),
                grant_ids: vec![grant.id.clone()],
                metadata_ids: vec!["metadata".into()],
                columns: vec!["amount".into()],
                all_data: false,
            },
        }),
        masking_versions: Some(vec![]),
    }
}
async fn observed(f: &Fixture) -> (String, i64) {
    use sqlx::Row;
    let state: String = sqlx::query_scalar("SELECT outcome FROM service_call WHERE id='atomic'")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let count = sqlx::query("SELECT COUNT(*) AS count FROM data_access_audit WHERE action='query'")
        .fetch_one(&f.pool)
        .await
        .unwrap()
        .get::<i64, _>("count");
    (state, count)
}

#[tokio::test]
async fn successful_access_and_call_commit_once_and_replay_preserves_original_evidence() {
    let (f, call, grant) = ready().await;
    let mut p = success(&call, &grant);
    let done = f.repo.finish_call(&p).await.unwrap();
    assert_eq!(observed(&f).await, ("succeeded".into(), 1));
    p.duration_ms = 99;
    assert_eq!(f.repo.finish_call(&p).await.unwrap(), done);
    assert_eq!(observed(&f).await.1, 1);
    p.access_audit.as_mut().unwrap().evidence.query_hash = "b".repeat(64);
    assert!(matches!(
        f.repo.finish_call(&p).await,
        Err(BackendError::Conflict(_))
    ));
    p.access_audit = None;
    p.masking_versions = None;
    assert!(f.repo.finish_call(&p).await.is_err());
}

#[tokio::test]
async fn revoked_original_grant_cannot_be_replaced_at_success_commit() {
    let (f, call, grant) = ready().await;
    let access = SqliteDataAccessRepo::new(f.pool.clone());
    let request = access
        .get_request("tenant", "request")
        .await
        .unwrap()
        .unwrap();
    access
        .decide(&DecideDataAccessParams {
            tenant_id: "tenant".into(),
            request_id: "request".into(),
            expected_version: request.resource.version,
            actor_id: "approver".into(),
            decision: DataAccessDecision::Revoke,
            reason: "Withdraw access".into(),
            expires_at: None,
            idempotency_key: "revoke-data".into(),
        })
        .await
        .unwrap();
    assert!(f.repo.finish_call(&success(&call, &grant)).await.is_err());
    assert_eq!(observed(&f).await, ("pending".into(), 0));
}

#[tokio::test]
async fn new_published_privacy_policy_invalidates_empty_mask_snapshot() {
    let (f, call, grant) = ready().await;
    crate::SqliteProductResourceRepo::new(f.pool.clone())
        .create(&dts_backend::CreateResourceParams {
            id: "policy".into(),
            tenant_id: "tenant".into(),
            kind: "masking_rule".into(),
            actor_id: "owner".into(),
            fields: dts_backend::ResourceWrite {
                code: "policy".into(),
                name: "Policy".into(),
                description: None,
                parent_id: None,
                status: "published".into(),
                properties: json!({}),
                references: vec![],
            },
        })
        .await
        .unwrap();
    assert!(f.repo.finish_call(&success(&call, &grant)).await.is_err());
    assert_eq!(observed(&f).await, ("pending".into(), 0));
}

#[tokio::test]
async fn failed_access_audit_insert_rolls_back_call_success() {
    let (f, call, grant) = ready().await;
    sqlx::raw_sql("CREATE TRIGGER reject_call_audit BEFORE INSERT ON data_access_audit WHEN NEW.action='query'
        BEGIN SELECT RAISE(ABORT,'audit fixture failure'); END;").execute(&f.pool).await.unwrap();
    assert!(f.repo.finish_call(&success(&call, &grant)).await.is_err());
    assert_eq!(observed(&f).await, ("pending".into(), 0));
    sqlx::query("DROP TRIGGER reject_call_audit")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(f.repo.finish_call(&success(&call, &grant)).await.is_ok());
    assert_eq!(observed(&f).await, ("succeeded".into(), 1));
}
