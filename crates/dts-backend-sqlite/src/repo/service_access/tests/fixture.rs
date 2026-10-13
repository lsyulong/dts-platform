use super::*;
use crate::SqliteProductResourceRepo;
use chrono::{Duration, Utc};
use dts_backend::{CreateResourceParams, ProductResourceRepo, ResourceWrite};
use serde_json::json;

pub struct Fixture {
    pub pool: SqlitePool,
    pub repo: SqliteServiceAccessRepo,
}
impl Fixture {
    pub async fn new(rate: u32) -> Self {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .unwrap();
        for sql in [
            include_str!("../../../../migrations/001_initial_schema.sql"),
            include_str!("../../../../migrations/002_product_resources.sql"),
            include_str!("../../../../migrations/004_product_external_refs.sql"),
            include_str!("../../../../migrations/011_metadata_catalog.sql"),
            include_str!("../../../../migrations/012_data_access.sql"),
            include_str!("../../../../migrations/015_service_access.sql"),
        ] {
            sqlx::raw_sql(sql).execute(&pool).await.unwrap();
        }
        sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('tenant','Tenant','tenant'),('other','Other','other');
            INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('owner','tenant','owner','owner@test','hash'),
            ('approver','tenant','approver','approver@test','hash'),('outsider','tenant','outsider','outsider@test','hash'),
            ('other-user','other','other','other@test','hash');
            INSERT INTO role(id,tenant_id,name) VALUES('approve-role','tenant','approvers'),('self-role','tenant','self'),('write-role','tenant','writers');
            INSERT INTO user_role(user_id,role_id) VALUES('approver','approve-role'),('owner','self-role'),('outsider','write-role');
            INSERT INTO role_permission(role_id,permission) VALUES('approve-role','services:approve'),('self-role','services:approve'),('write-role','services:write');")
            .execute(&pool).await.unwrap();
        SqliteProductResourceRepo::new(pool.clone()).create(&CreateResourceParams {id:"api".into(),tenant_id:"tenant".into(),kind:"data_api".into(),actor_id:"owner".into(),
            fields:ResourceWrite {code:"api".into(),name:"API".into(),description:None,parent_id:None,status:"published".into(),
                properties:json!({"rate_limit":rate,"publication":{"format_version":1,"executor":"data_api_detail_v1","tenant_id":"tenant","source_id":"api","source_version":1,"api":{"rate_limit":rate}}}),references:vec![]}}).await.unwrap();
        Self {
            repo: SqliteServiceAccessRepo::new(pool.clone()),
            pool,
        }
    }
    pub async fn application(&self, id: &str, name: &str) -> ProductResource {
        self.repo
            .save_application(&SaveServiceApplicationParams {
                id: id.into(),
                tenant_id: "tenant".into(),
                actor_id: "owner".into(),
                expected_version: None,
                code: id.into(),
                name: name.into(),
                description: None,
                owner_id: "owner".into(),
                allowed_origins: vec![],
                credential_expires_at: Utc::now() + Duration::days(5),
            })
            .await
            .unwrap()
    }
    pub async fn subscription(&self, app: &str) -> ProductResource {
        self.repo
            .save_subscription(&SaveServiceSubscriptionParams {
                id: "subscription".into(),
                tenant_id: "tenant".into(),
                actor_id: "owner".into(),
                expected_version: None,
                code: "sub".into(),
                name: "Subscription%".into(),
                description: None,
                application_id: app.into(),
                api_id: "api".into(),
                api_version: 1,
                purpose: "Needed for reporting".into(),
                expires_at: Utc::now() + Duration::days(3),
            })
            .await
            .unwrap()
    }
    pub fn decision(
        &self,
        actor: &str,
        version: i64,
        decision: ServiceSubscriptionDecision,
        key: &str,
    ) -> DecideServiceSubscriptionParams {
        DecideServiceSubscriptionParams {
            tenant_id: "tenant".into(),
            actor_id: actor.into(),
            subscription_id: "subscription".into(),
            expected_version: version,
            decision,
            reason: "Reviewed".into(),
            expires_at: None,
            idempotency_key: key.into(),
        }
    }
    pub async fn approve(&self, app: &str) {
        let draft = self.subscription(app).await;
        let pending = self
            .repo
            .decide_subscription(&self.decision(
                "owner",
                draft.version,
                ServiceSubscriptionDecision::Submit,
                "submit",
            ))
            .await
            .unwrap()
            .subscription;
        self.repo
            .decide_subscription(&self.decision(
                "approver",
                pending.version,
                ServiceSubscriptionDecision::Approve,
                "approve",
            ))
            .await
            .unwrap();
    }
    pub async fn credential(&self, app: &str, id: &str) -> ServiceCredentialSummary {
        let app = self
            .repo
            .get_application("tenant", "owner", app)
            .await
            .unwrap()
            .unwrap();
        self.repo
            .issue_credential(&IssueServiceCredentialParams {
                tenant_id: "tenant".into(),
                actor_id: "owner".into(),
                application_id: app.resource.id,
                expected_version: app.resource.version,
                idempotency_key: id.into(),
                credential_id: id.into(),
                verifier: "a".repeat(64),
                requested_expires_at: None,
                expires_at: app.credential_expires_at,
            })
            .await
            .unwrap()
            .credential
    }
    pub fn reserve(
        &self,
        credential: &ServiceCredentialSummary,
        id: &str,
    ) -> ReserveServiceCallParams {
        ReserveServiceCallParams {
            call_id: id.into(),
            tenant_id: "tenant".into(),
            credential_id: credential.id.clone(),
            credential_version: credential.version,
            credential_verifier: "a".repeat(64),
            api_id: "api".into(),
            origin: None,
        }
    }
    pub fn filter(&self, actor: &str) -> ServiceResourceFilter {
        ServiceResourceFilter {
            tenant_id: "tenant".into(),
            actor_id: actor.into(),
            approvals: false,
            status: None,
            q: None,
            limit: 20,
            offset: 0,
        }
    }
    pub fn calls(&self, actor: &str) -> ServiceCallFilter {
        ServiceCallFilter {
            tenant_id: "tenant".into(),
            actor_id: actor.into(),
            application_id: None,
            api_id: None,
            limit: 20,
            offset: 0,
        }
    }
}
