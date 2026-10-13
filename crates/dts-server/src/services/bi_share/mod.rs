// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::{
        auth_svc::AuthService,
        product_exports::{ExportArtifact, ExportFormat, ExportRequest, ResultExportService},
        product_jobs::{ProductJobService, ProductJobView, ResultAccess},
    },
};
use chrono::{DateTime, Utc};
use dts_backend::repo::{
    bi_share::*,
    product_job::{ProductJobNodeStatus, ProductJobStatus},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBiShareRequest {
    pub expected_version: i64,
    pub job_id: String,
    pub node_id: String,
    pub recipient_id: String,
    pub expires_at: DateTime<Utc>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeBiShareRequest {
    pub expected_version: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiShareListQuery {
    pub report_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
#[derive(Serialize)]
pub struct SharedBiReport {
    pub share: BiShare,
    pub job: ProductJobView,
}

#[derive(Clone)]
pub struct BiShareService {
    shares: Arc<dyn BiShareRepo>,
    jobs: Arc<ProductJobService>,
    exports: Arc<ResultExportService>,
    auth: AuthService,
}
impl BiShareService {
    pub fn new(
        shares: Arc<dyn BiShareRepo>,
        jobs: Arc<ProductJobService>,
        exports: Arc<ResultExportService>,
        auth: AuthService,
    ) -> Self {
        Self {
            shares,
            jobs,
            exports,
            auth,
        }
    }
    async fn authorize(&self, actor: &AuthIdentity, write: bool) -> Result<(), AppError> {
        self.auth.validate_identity(actor).await?;
        self.auth.authorize(actor, "bi:read").await?;
        if write {
            self.auth.authorize(actor, "bi:write").await?;
        }
        Ok(())
    }
    pub async fn create(
        &self,
        actor: &AuthIdentity,
        report_id: &str,
        request: CreateBiShareRequest,
    ) -> Result<BiShare, AppError> {
        self.authorize(actor, true).await?;
        let result = self.jobs.get(actor, &request.job_id).await?;
        require_result(
            &result,
            report_id,
            request.expected_version,
            &request.node_id,
        )?;
        // Repository verifies active real recipient/owner, exact current
        // publication, successful node and DB-clock expiry in one transaction.
        Ok(self
            .shares
            .create(&CreateBiShareParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                owner_id: actor.sub.clone(),
                recipient_id: request.recipient_id,
                report_id: report_id.into(),
                report_version: request.expected_version,
                job_id: request.job_id,
                node_id: request.node_id,
                expires_at: request.expires_at,
            })
            .await?)
    }
    pub async fn list(
        &self,
        actor: &AuthIdentity,
        query: BiShareListQuery,
    ) -> Result<BiSharePage, AppError> {
        self.authorize(actor, false).await?;
        Ok(self
            .shares
            .list(&BiShareFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                report_id: query.report_id,
                limit: query.limit.unwrap_or(10),
                offset: query.offset.unwrap_or(0),
            })
            .await?)
    }
    pub async fn get(&self, actor: &AuthIdentity, id: &str) -> Result<BiShare, AppError> {
        self.authorize(actor, false).await?;
        self.shares
            .get(&lookup(actor, id))
            .await?
            .ok_or_else(|| AppError::NotFound("分享不属于当前真实用户".into()))
    }
    pub async fn open(&self, actor: &AuthIdentity, id: &str) -> Result<SharedBiReport, AppError> {
        self.authorize(actor, false).await?;
        let share = self.shares.open(&lookup(actor, id)).await?;
        // A share selects a saved result; it does not delegate the creator's
        // data grant, identity or output policy to the recipient.
        let job = self.jobs.get(actor, &share.job_id).await?;
        require_result(&job, &share.report_id, share.report_version, &share.node_id)?;
        let final_share = self.shares.open(&lookup(actor, id)).await?;
        if final_share != share {
            return Err(AppError::Conflict("读取期间分享已变化".into()));
        }
        self.authorize(actor, false).await?;
        Ok(SharedBiReport {
            share: final_share,
            job,
        })
    }
    pub async fn revoke(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: RevokeBiShareRequest,
    ) -> Result<BiShare, AppError> {
        self.authorize(actor, true).await?;
        Ok(self
            .shares
            .revoke(&RevokeBiShareParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                share_id: id.into(),
                expected_version: request.expected_version,
            })
            .await?)
    }
    pub async fn export(
        &self,
        actor: &AuthIdentity,
        id: &str,
        format: ExportFormat,
    ) -> Result<ExportArtifact, AppError> {
        let share = self.open(actor, id).await?.share;
        let artifact = self
            .exports
            .export(
                actor,
                &share.job_id,
                ExportRequest {
                    node_id: share.node_id.clone(),
                    expected_source_version: share.report_version,
                    format,
                },
            )
            .await?;
        let final_share = self.shares.open(&lookup(actor, id)).await?;
        if final_share != share {
            return Err(AppError::Conflict("导出期间分享已撤销、到期或变化".into()));
        }
        self.authorize(actor, false).await?;
        Ok(artifact)
    }
}
fn lookup(actor: &AuthIdentity, id: &str) -> BiShareLookup {
    BiShareLookup {
        tenant_id: actor.tenant_id.clone(),
        actor_id: actor.sub.clone(),
        share_id: id.into(),
    }
}
fn require_result(
    job: &ProductJobView,
    report: &str,
    version: i64,
    node: &str,
) -> Result<(), AppError> {
    if job.source_kind != "bi_report"
        || job.source_id != report
        || job.source_version != version
        || job.status != ProductJobStatus::Succeeded
    {
        return Err(AppError::Conflict(
            "分享须选择当前报表版本的实际成功运行".into(),
        ));
    }
    if job.result_access != ResultAccess::Authorized
        || !job.nodes.iter().any(|actual| {
            actual.id == node
                && actual.status == ProductJobNodeStatus::Succeeded
                && actual.result.is_some()
        })
    {
        return Err(AppError::Forbidden(
            "当前用户数据权限或脱敏策略不允许读取所选真实结果".into(),
        ));
    }
    Ok(())
}
