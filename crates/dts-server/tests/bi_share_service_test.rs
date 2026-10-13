// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
mod bi_share_service_support;
mod common;
#[path = "product_job_service_support/masking.rs"]
mod masking;
use bi_share_service_support::*;
use chrono::{Duration, Utc};
use dts_backend::{
    ResourceWrite, UpdateResourceParams,
    repo::{
        bi_share::BiShareStatus,
        product_job::{ProductJobLookup, ProductJobStatus},
    },
};
use dts_server::services::{
    bi_share::{CreateBiShareRequest, RevokeBiShareRequest},
    product_exports::{ExportFormat, ExportRequest},
};

fn request(f: &Fixture, recipient: &str) -> CreateBiShareRequest {
    CreateBiShareRequest {
        expected_version: f.report.version,
        job_id: f.job_id.clone(),
        node_id: "report".into(),
        recipient_id: recipient.into(),
        expires_at: Utc::now() + Duration::hours(1),
    }
}

#[actix_web::test]
async fn owner_and_real_recipient_can_export_authorized_precision_without_definition_or_receipt() {
    let f = fixture().await;
    let (recipient, _) = reader(&f, "recipient", true).await;
    let (outsider, _) = reader(&f, "outsider", true).await;
    let shared = f
        .shares
        .create(&f.actor, &f.report.id, request(&f, &recipient.sub))
        .await
        .unwrap();
    assert_eq!(shared.status, BiShareStatus::Active);
    let opened = f.shares.open(&recipient, &shared.id).await.unwrap();
    assert_eq!(opened.share.report_version, f.report.version);
    assert_eq!(opened.job.status, ProductJobStatus::Succeeded);
    assert!(f.shares.get(&outsider, &shared.id).await.is_err());
    assert!(f.shares.open(&outsider, &shared.id).await.is_err());
    let exported = f
        .shares
        .export(&recipient, &shared.id, ExportFormat::Json)
        .await
        .unwrap();
    let actual: serde_json::Value = serde_json::from_slice(exported.as_bytes()).unwrap();
    assert_eq!(actual["rows"][0]["total_amount"], "9007199254740991.25");
    assert_eq!(actual["row_count"], 1);
    assert_eq!(exported.row_count, 1);
    assert!(actual.get("definition_snapshot").is_none());
    assert!(actual.get("privacy").is_none());
    let csv = f
        .exports
        .export(
            &f.actor,
            &f.job_id,
            ExportRequest {
                node_id: "report".into(),
                expected_source_version: f.report.version,
                format: ExportFormat::Csv,
            },
        )
        .await
        .unwrap();
    assert!(
        std::str::from_utf8(csv.as_bytes())
            .unwrap()
            .contains("'9007199254740991.25")
    );
    assert_eq!(csv.csv_encoding.unwrap().null_token, "\\N");
    f.shares
        .revoke(
            &f.actor,
            &shared.id,
            RevokeBiShareRequest {
                expected_version: shared.version,
            },
        )
        .await
        .unwrap();
    assert!(f.shares.open(&recipient, &shared.id).await.is_err());
    assert!(
        f.shares
            .export(&recipient, &shared.id, ExportFormat::Json)
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn share_does_not_delegate_creator_data_grants_and_current_revocation_blocks_exports() {
    let f = fixture().await;
    let (recipient, role) = reader(&f, "restricted", false).await;
    let shared = f
        .shares
        .create(&f.actor, &f.report.id, request(&f, &recipient.sub))
        .await
        .unwrap();
    assert!(f.shares.open(&recipient, &shared.id).await.is_err());
    assert!(
        f.shares
            .export(&recipient, &shared.id, ExportFormat::Json)
            .await
            .is_err()
    );
    f.ctx
        .role_repo
        .update(
            &role,
            &dts_backend::types::UpdateRoleParams {
                name: None,
                permissions: Some(vec![
                    "bi:read".into(),
                    "data_security:read".into(),
                    "data_security:write".into(),
                    "data_security:all_data".into(),
                ]),
            },
        )
        .await
        .unwrap();
    assert!(
        f.shares.open(&recipient, &shared.id).await.is_err(),
        "ordinary-role all_data does not delegate real administrator authority"
    );
    let approved = grant_reader(&f, &recipient).await;
    let opened = f.shares.open(&recipient, &shared.id).await;
    assert!(
        opened.is_ok(),
        "a real approved recipient grant must release the shared result: {:?}",
        opened.err()
    );
    access_service(&f).action(&f.actor,&approved.id,"revoke",dts_server::services::product::ResourceAction{expected_version:approved.version,payload:serde_json::json!({"idempotency_key":uuid::Uuid::new_v4().to_string(),"reason":"The actual recipient data grant was withdrawn"})})
        .await
        .unwrap();
    assert!(
        f.shares
            .export(&recipient, &shared.id, ExportFormat::Json)
            .await
            .is_err()
    );
    let raw = f
        .ctx
        .product_job_repo
        .get(&ProductJobLookup {
            tenant_id: f.actor.tenant_id.clone(),
            job_id: f.job_id.clone(),
            actor_id: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(raw.status, ProductJobStatus::Succeeded);
    assert!(raw.nodes[0].result.is_some());
}

#[actix_web::test]
async fn changed_publication_invalid_target_and_stale_version_cannot_release_a_share() {
    let f = fixture().await;
    let (recipient, _) = reader(&f, "receiver", true).await;
    let mut invalid = request(&f, &recipient.sub);
    invalid.node_id = "missing".into();
    assert!(
        f.shares
            .create(&f.actor, &f.report.id, invalid)
            .await
            .is_err()
    );
    invalid = request(&f, &recipient.sub);
    invalid.expected_version = f.report.version + 1;
    assert!(
        f.shares
            .create(&f.actor, &f.report.id, invalid)
            .await
            .is_err()
    );
    invalid = request(&f, &f.actor.sub);
    assert!(
        f.shares
            .create(&f.actor, &f.report.id, invalid)
            .await
            .is_err()
    );
    let shared = f
        .shares
        .create(&f.actor, &f.report.id, request(&f, &recipient.sub))
        .await
        .unwrap();
    f.ctx
        .product_resource_repo
        .update(
            &f.actor.tenant_id,
            "bi_report",
            &f.report.id,
            &UpdateResourceParams {
                expected_version: f.report.version,
                actor_id: f.actor.sub.clone(),
                fields: ResourceWrite {
                    code: f.report.code.clone(),
                    name: f.report.name.clone(),
                    description: f.report.description.clone(),
                    parent_id: f.report.parent_id.clone(),
                    status: "inactive".into(),
                    properties: f.report.properties.clone(),
                    references: f.report.references.clone(),
                },
            },
        )
        .await
        .unwrap();
    assert!(f.shares.open(&recipient, &shared.id).await.is_err());
    assert!(
        f.shares
            .export(&recipient, &shared.id, ExportFormat::Json)
            .await
            .is_err()
    );
    let mut foreign = recipient.clone();
    foreign.tenant_id = "another-tenant".into();
    assert!(f.shares.get(&foreign, &shared.id).await.is_err());
}
