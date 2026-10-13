use super::*;
use crate::{api::query::QueryAccess, models::auth::AuthIdentity};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use dts_backend::{BackendError, ProductResource, ResourcePage, repo::metadata_catalog::*};
use std::sync::{Arc, Mutex};

pub(super) struct FixtureRepo {
    pub(super) authority: Mutex<DataAccessAuthority>,
    pub(super) grants: Mutex<Vec<DataAccessGrant>>,
    pub(super) events: Mutex<Vec<(AccessOutcome, DataAccessEvidence)>>,
}
pub(super) struct FixtureMetadata {
    pub(super) binding: Mutex<Option<MetadataEngineBinding>>,
}
fn unsupported<T>() -> Result<T, BackendError> {
    Err(BackendError::Configuration(
        "此查询策略测试不使用业务写入".into(),
    ))
}

#[async_trait]
impl DataAccessRepo for FixtureRepo {
    async fn actor_authority(
        &self,
        tenant: &str,
        actor: &str,
    ) -> Result<DataAccessAuthority, BackendError> {
        Ok(if tenant == "tenant-a" && actor == "user-a" {
            self.authority.lock().unwrap().clone()
        } else {
            DataAccessAuthority::default()
        })
    }
    async fn active_grants(
        &self,
        _: &str,
        _: &str,
        _: DateTime<Utc>,
    ) -> Result<Vec<DataAccessGrant>, BackendError> {
        Ok(self.grants.lock().unwrap().clone())
    }
    async fn record_access(&self, params: &RecordDataAccessParams) -> Result<(), BackendError> {
        self.events
            .lock()
            .unwrap()
            .push((params.outcome, params.evidence.clone()));
        Ok(())
    }
    async fn save_request(
        &self,
        _: &SaveDataAccessRequestParams,
    ) -> Result<ProductResource, BackendError> {
        unsupported()
    }
    async fn get_request(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<DataAccessRequest>, BackendError> {
        unsupported()
    }
    async fn decide(
        &self,
        _: &DecideDataAccessParams,
    ) -> Result<DataAccessDecisionResult, BackendError> {
        unsupported()
    }
    async fn list_requests(
        &self,
        _: &DataAccessRequestFilter,
    ) -> Result<ResourcePage, BackendError> {
        unsupported()
    }
    async fn list_audit(
        &self,
        _: &DataAccessAuditFilter,
    ) -> Result<DataAccessAuditPage, BackendError> {
        unsupported()
    }
}

#[async_trait]
impl MetadataCatalogRepo for FixtureMetadata {
    async fn apply_snapshot(
        &self,
        _: &MetadataApplyParams,
    ) -> Result<MetadataApplyResult, BackendError> {
        unsupported()
    }
    async fn verified_binding(
        &self,
        tenant: &str,
        id: &str,
        version: i64,
    ) -> Result<Option<MetadataEngineBinding>, BackendError> {
        Ok(self.binding.lock().unwrap().clone().filter(|binding| {
            tenant == binding.tenant_id && id == "meta-1" && version == binding.object_version
        }))
    }
}

pub(super) fn fixture() -> (
    QueryAuthorizationService,
    Arc<FixtureRepo>,
    Arc<FixtureMetadata>,
    AuthIdentity,
) {
    let target = target();
    let now = Utc::now();
    let grant = DataAccessGrant {
        id: "grant-1".into(),
        tenant_id: "tenant-a".into(),
        request_id: "request-1".into(),
        request_version: 3,
        grantee_id: "user-a".into(),
        target: target.clone(),
        columns: vec!["id".into()],
        granted_by: "reviewer".into(),
        granted_at: now,
        expires_at: now + Duration::days(1),
        revoked_at: None,
        revoked_by: None,
    };
    let repo = Arc::new(FixtureRepo {
        authority: Mutex::new(DataAccessAuthority {
            active: true,
            can_approve: false,
            all_data: false,
        }),
        grants: Mutex::new(vec![grant]),
        events: Mutex::new(vec![]),
    });
    let metadata = Arc::new(FixtureMetadata {
        binding: Mutex::new(Some(target.binding)),
    });
    let service = QueryAuthorizationService::new(
        repo.clone(),
        metadata.clone(),
        QueryAccess {
            tenant_id: Some("tenant-a".into()),
        },
    );
    (
        service,
        repo,
        metadata,
        AuthIdentity {
            sub: "user-a".into(),
            tenant_id: "tenant-a".into(),
            username: "A".into(),
        },
    )
}

#[tokio::test]
async fn actual_referenced_columns_drive_permission_and_denial_audit() {
    let (service, repo, _, actor) = fixture();
    let authorized = service
        .authorize(&actor, "SELECT id FROM warehouse.main.orders LIMIT 10")
        .await
        .unwrap();
    assert_eq!(authorized.evidence().columns, ["id"]);
    for sql in [
        "SELECT secret FROM warehouse.main.orders",
        "SELECT id FROM warehouse.main.orders WHERE secret='x'",
        "SELECT id FROM warehouse.main.orders ORDER BY secret",
        "SELECT * FROM warehouse.main.orders",
        "SELECT COUNT(*) FROM warehouse.main.orders",
    ] {
        assert!(service.authorize(&actor, sql).await.is_err());
    }
    assert_eq!(repo.events.lock().unwrap().len(), 5);
    assert!(
        repo.events
            .lock()
            .unwrap()
            .iter()
            .all(
                |(outcome, evidence)| matches!(outcome, AccessOutcome::Denied)
                    && evidence.query_hash.len() == 64
                    && evidence.columns.is_empty()
            )
    );
}

#[tokio::test]
async fn reauthorization_rejects_revoked_expired_foreign_or_replaced_grant() {
    let (service, repo, _, actor) = fixture();
    let query = service
        .authorize(&actor, "SELECT id FROM warehouse.main.orders")
        .await
        .unwrap();
    assert!(service.reauthorize(&actor, &query).await.is_ok());
    repo.grants.lock().unwrap()[0].revoked_at = Some(Utc::now());
    assert!(service.reauthorize(&actor, &query).await.is_err());
    repo.grants.lock().unwrap()[0].revoked_at = None;
    repo.grants.lock().unwrap()[0].expires_at = Utc::now() - Duration::seconds(1);
    assert!(service.reauthorize(&actor, &query).await.is_err());
    repo.grants.lock().unwrap()[0].expires_at = Utc::now() + Duration::days(1);
    repo.grants.lock().unwrap()[0].id = "replacement".into();
    assert!(service.reauthorize(&actor, &query).await.is_err());
    let foreign = AuthIdentity {
        sub: "someone-else".into(),
        ..actor.clone()
    };
    assert!(service.reauthorize(&foreign, &query).await.is_err());
    assert!(
        service
            .record_access(&foreign, &query, AccessOutcome::Succeeded)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn current_metadata_proof_and_active_identity_are_mandatory() {
    let (service, repo, metadata, actor) = fixture();
    let query = service
        .authorize(&actor, "SELECT id FROM warehouse.main.orders")
        .await
        .unwrap();
    metadata
        .binding
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .object_version = 2;
    assert!(service.reauthorize(&actor, &query).await.is_err());
    *metadata.binding.lock().unwrap() = Some(target().binding);
    repo.authority.lock().unwrap().active = false;
    assert!(
        service
            .authorize(&actor, "SELECT id FROM warehouse.main.orders")
            .await
            .is_err()
    );
    repo.authority.lock().unwrap().active = true;
    *metadata.binding.lock().unwrap() = None;
    assert!(
        service
            .authorize(&actor, "SELECT id FROM warehouse.main.orders")
            .await
            .is_err()
    );
    assert!(service.visible_schemas(&actor).await.unwrap().is_empty());
}

#[tokio::test]
async fn explicit_admin_bypass_still_requires_tenant_active_and_read_only_query() {
    let (service, repo, _, actor) = fixture();
    repo.authority.lock().unwrap().all_data = true;
    let query = service
        .authorize(&actor, "WITH a AS(SELECT 1) SELECT * FROM a")
        .await
        .unwrap();
    assert!(query.evidence().all_data);
    assert!(
        service
            .authorize(&actor, "DROP TABLE orders")
            .await
            .is_err()
    );
    assert!(
        service
            .authorize(&actor, "SELECT * FROM read_csv('secret.csv')")
            .await
            .is_err()
    );
    repo.authority.lock().unwrap().all_data = false;
    assert!(service.reauthorize(&actor, &query).await.is_err());
    let foreign = AuthIdentity {
        tenant_id: "tenant-b".into(),
        ..actor
    };
    assert!(service.has_all_data(&foreign).await.is_err());
}

#[tokio::test]
async fn grants_combine_only_for_same_frozen_target_and_query_text_stays_exact() {
    let (service, repo, _, actor) = fixture();
    let mut second = repo.grants.lock().unwrap()[0].clone();
    second.id = "grant-2".into();
    second.columns = vec!["secret".into()];
    repo.grants.lock().unwrap().push(second);
    let sql = "SELECT id FROM warehouse.main.orders WHERE secret='x''; -- \\ slash'";
    let authorized = service.authorize(&actor, sql).await.unwrap();
    assert_eq!(authorized.sql(), sql);
    assert_eq!(authorized.evidence().columns, ["id", "secret"]);
    service
        .record_access(&actor, &authorized, AccessOutcome::Succeeded)
        .await
        .unwrap();
    let events = repo.events.lock().unwrap();
    assert_eq!(events[0].1.grant_ids, ["grant-1", "grant-2"]);
    drop(events);
    assert_eq!(
        service.visible_schemas(&actor).await.unwrap()[0].tables[0].name,
        "orders"
    );
}
