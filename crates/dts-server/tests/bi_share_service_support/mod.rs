// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
use crate::{common, masking};
use dts_backend::{
    CreateResourceParams, ProductResource, ProductResourceFilter, ResourceWrite,
    repo::product_job::ProductJobStatus,
};
use dts_server::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        bi_report::{BiReportService, BiRunRequest},
        bi_share::BiShareService,
        data_access::{DataAccessService, QueryAuthorizationService},
        metadata::{MetadataRunRequest, MetadataService},
        product::{ResourceAction, ResourceInput},
        product_exports::ResultExportService,
        product_jobs::{ProductJobService, ReadOnlyQueryExecutor},
        query_svc::{ColumnMeta, QueryResult},
    },
};
use serde_json::{Value, json};
use std::sync::Arc;

struct Observations;
#[async_trait::async_trait]
impl ReadOnlyQueryExecutor for Observations {
    async fn execute(&self, sql: &str) -> Result<QueryResult, AppError> {
        // Explicit deterministic observations; this fixture verifies real SQLite
        // control-plane paths, and is not a live SwanLake POC.
        if sql.contains("information_schema") {
            let rows = vec![
                json!({"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE","column_name":"region","data_type":"VARCHAR","is_nullable":"NO","ordinal_position":1}),
                json!({"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE","column_name":"amount","data_type":"DECIMAL(18,2)","is_nullable":"YES","ordinal_position":2}),
            ];
            return Ok(QueryResult {
                columns: [
                    "table_catalog",
                    "schema_name",
                    "table_name",
                    "table_type",
                    "column_name",
                    "data_type",
                    "is_nullable",
                    "ordinal_position",
                ]
                .into_iter()
                .map(|name| ColumnMeta {
                    name: name.into(),
                    data_type: "fixture".into(),
                    is_numeric: false,
                })
                .collect(),
                row_count: rows.len(),
                rows,
                suggested_chart: None,
            });
        }
        Ok(QueryResult {
            columns: vec![
                ColumnMeta {
                    name: "region".into(),
                    data_type: "Utf8".into(),
                    is_numeric: false,
                },
                ColumnMeta {
                    name: "total_amount".into(),
                    data_type: "Decimal128(38,2)".into(),
                    is_numeric: true,
                },
            ],
            rows: vec![json!({"region":"华东","total_amount":"9007199254740991.25"})],
            row_count: 1,
            suggested_chart: None,
        })
    }
}
async fn resource(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    kind: &str,
    properties: Value,
) -> ProductResource {
    let id = uuid::Uuid::new_v4().to_string();
    ctx.product_resource_repo
        .create(&CreateResourceParams {
            id: id.clone(),
            tenant_id: actor.tenant_id.clone(),
            kind: kind.into(),
            actor_id: actor.sub.clone(),
            fields: ResourceWrite {
                code: id,
                name: "共享导出真实合同测试".into(),
                description: None,
                parent_id: None,
                status: "draft".into(),
                properties,
                references: vec![],
            },
        })
        .await
        .unwrap()
}
pub struct Fixture {
    pub ctx: common::TestBackend,
    pub actor: AuthIdentity,
    pub worker: Arc<ProductJobService>,
    pub exports: Arc<ResultExportService>,
    pub shares: BiShareService,
    pub report: ProductResource,
    pub job_id: String,
}
pub async fn fixture() -> Fixture {
    let ctx = common::setup_test_backend().await.unwrap();
    let token = common::admin_token(&ctx).await;
    let actor = ctx.auth.authenticate(&token).await.unwrap();
    let access = QueryAccess {
        tenant_id: Some(actor.tenant_id.clone()),
    };
    let guard = Arc::new(
        QueryAuthorizationService::new(
            ctx.data_access_repo.clone(),
            ctx.metadata_catalog_repo.clone(),
            access.clone(),
        )
        .with_job_repo(ctx.product_job_repo.clone()),
    );
    let worker = Arc::new(
        ProductJobService::with_executor(
            ctx.product_job_repo.clone(),
            ctx.product_resource_repo.clone(),
            ctx.auth.clone(),
            Arc::new(Observations),
            access.clone(),
        )
        .with_metadata_repo(ctx.metadata_catalog_repo.clone())
        .with_query_authorization(guard)
        .with_masking(masking::service(&ctx)),
    );
    let collector = MetadataService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.auth.clone(),
        access.clone(),
    );
    let collection = resource(
        &ctx,
        &actor,
        "metadata_collection",
        json!({"source_type":"swanlake"}),
    )
    .await;
    collector
        .enqueue(
            &actor,
            &collection.id,
            MetadataRunRequest {
                expected_version: collection.version,
                idempotency_key: "share-registry-fixture".into(),
            },
        )
        .await
        .unwrap();
    worker.run_once().await.unwrap();
    let metadata = ctx
        .product_resource_repo
        .list(&ProductResourceFilter {
            tenant_id: actor.tenant_id.clone(),
            kind: "metadata_object".into(),
            q: None,
            status: None,
            created_by: None,
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap()
        .data
        .remove(0);
    let service = BiReportService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        ctx.auth.clone(),
        access,
    );
    let source=resource(&ctx,&actor,"bi_report",json!({"metadata_object_id":metadata.id,"dimensions":["region"],"measures":[{"name":"total_amount","expression":"amount","aggregate":"sum"}],"row_limit":2,
        "chart_config":{"type":"table","title":"精度报表","x_field":"region","y_field":"total_amount","legend":false}})).await;
    let published = service
        .publish(&actor, &source.id, source.version)
        .await
        .unwrap();
    let version = published["resource"]["version"].as_i64().unwrap();
    let job = service
        .enqueue(
            &actor,
            &source.id,
            BiRunRequest {
                expected_version: version,
                idempotency_key: "share-real-report".into(),
            },
        )
        .await
        .unwrap();
    worker.run_once().await.unwrap();
    assert_eq!(
        worker.get(&actor, &job.job.id).await.unwrap().status,
        ProductJobStatus::Succeeded
    );
    let report = ctx
        .product_resource_repo
        .get(&actor.tenant_id, "bi_report", &source.id)
        .await
        .unwrap()
        .unwrap();
    let exports = Arc::new(ResultExportService::new(worker.clone()));
    let shares = BiShareService::new(
        ctx.bi_share_repo.clone(),
        worker.clone(),
        exports.clone(),
        ctx.auth.clone(),
    );
    Fixture {
        ctx,
        actor,
        worker,
        exports,
        shares,
        report,
        job_id: job.job.id,
    }
}

pub async fn reader(f: &Fixture, name: &str, granted: bool) -> (AuthIdentity, String) {
    let user_id = uuid::Uuid::new_v4().to_string();
    f.ctx
        .user_repo
        .create(&dts_backend::types::CreateUserParams {
            id: user_id.clone(),
            tenant_id: f.actor.tenant_id.clone(),
            username: name.into(),
            email: format!("{name}@example.test"),
            password_hash: "test-only-password-hash".into(),
            display_name: None,
        })
        .await
        .unwrap();
    let role_id = uuid::Uuid::new_v4().to_string();
    let permissions = vec![
        "bi:read".into(),
        "data_security:read".into(),
        "data_security:write".into(),
    ];
    f.ctx
        .role_repo
        .create_with_permissions(&dts_backend::types::CreateRoleParams {
            id: role_id.clone(),
            tenant_id: f.actor.tenant_id.clone(),
            name: format!("{name}-reader"),
            description: String::new(),
            is_system: false,
            permissions,
        })
        .await
        .unwrap();
    f.ctx
        .user_repo
        .assign_role(&user_id, &role_id)
        .await
        .unwrap();
    let token = common::test_token(&f.ctx, &user_id).await;
    let actor = f.ctx.auth.authenticate(&token).await.unwrap();
    if granted {
        grant_reader(f, &actor).await;
    }
    (actor, role_id)
}

pub fn access_service(f: &Fixture) -> DataAccessService {
    DataAccessService::new(
        f.ctx.data_access_repo.clone(),
        f.ctx.product_resource_repo.clone(),
        f.ctx.metadata_catalog_repo.clone(),
        QueryAccess {
            tenant_id: Some(f.actor.tenant_id.clone()),
        },
    )
}

pub async fn grant_reader(f: &Fixture, reader: &AuthIdentity) -> ProductResource {
    // Ordinary report readers use a real approved grant. A share or an all_data
    // string on a non-administrator role must never manufacture query authority.
    let service = access_service(f);
    let draft=service.save_request(reader,ResourceInput{code:format!("share_read_{}",uuid::Uuid::new_v4().simple()),name:"已分享报表的实际数据权限".into(),description:None,parent_id:None,properties:json!({"object_id":f.report.properties["metadata_object_id"],"columns":["region","amount"],"purpose":"Read the actual shared report under my own column grant"})}).await.expect("real current metadata grant request");
    let pending = service
        .action(
            reader,
            &draft.id,
            "submit",
            ResourceAction {
                expected_version: draft.version,
                payload: json!({"idempotency_key":uuid::Uuid::new_v4().to_string()}),
            },
        )
        .await
        .expect("actual owner submit");
    let approved=service.action(&f.actor,&draft.id,"approve",ResourceAction{expected_version:pending.request.version,payload:json!({"idempotency_key":uuid::Uuid::new_v4().to_string(),"reason":"Approved for the scoped report fields"})}).await.expect("real separate administrator approval");
    assert_eq!(approved.grant.as_ref().unwrap().grantee_id, reader.sub);
    let authority = f
        .ctx
        .data_access_repo
        .actor_authority(&reader.tenant_id, &reader.sub)
        .await
        .unwrap();
    assert!(
        !authority.all_data,
        "ordinary readers must use grants rather than administrator bypass"
    );
    approved.request
}
