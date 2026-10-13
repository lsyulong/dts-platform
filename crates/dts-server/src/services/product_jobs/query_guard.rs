// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::ProductJobService;
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::{
        data_access::{AccessOutcome, AuthorizedQuery, QueryAuthorizationService},
        data_masking::{MaskingPlan, MaskingReceipt, MaskingService},
        query_svc::QueryResult,
    },
};
use dts_backend::repo::product_job::ProductJob;
use serde_json::Value;
use std::sync::Arc;

pub(super) enum GuardedNodeOutput {
    Query(QueryResult, Option<MaskingReceipt>),
    Api(Value),
    Recognition(dts_backend::repo::sensitive_findings::SensitiveScanResult),
}

pub(super) struct JobQueryGuard {
    actor: AuthIdentity,
    service: Option<Arc<QueryAuthorizationService>>,
    query: Option<AuthorizedQuery>,
    masking: Option<Arc<MaskingService>>,
    plan: Option<MaskingPlan>,
}

impl JobQueryGuard {
    pub async fn prepare(
        service: &ProductJobService,
        job: &ProductJob,
        node_id: &str,
        sql: &str,
    ) -> Result<Self, AppError> {
        let user = service
            .auth
            .user_repo
            .find_detail_by_id(&job.actor_id)
            .await?
            .filter(|user| user.tenant_id == job.tenant_id)
            .ok_or_else(|| AppError::Unauthorized("查询提交者身份已不存在".into()))?;
        let actor = AuthIdentity {
            sub: user.id,
            tenant_id: user.tenant_id,
            username: user.username,
        };
        // This branch only follows the fixed metadata compiler, never a SQL
        // document's user-controlled tag or properties.
        if job.source_kind == "metadata_collection" {
            return Ok(Self {
                actor,
                service: None,
                query: None,
                masking: None,
                plan: None,
            });
        }
        let authorization = service.query_authorization.clone().ok_or_else(|| {
            AppError::BadRequest("数据查询授权服务未配置，禁止执行用户 SQL".into())
        })?;
        let query = if job.source_kind == "quality_task" {
            authorization
                .authorize_quality(&actor, job, node_id, sql)
                .await?
        } else if job.source_kind == "recognition_task" {
            authorization
                .authorize_recognition(&actor, job, node_id, sql)
                .await?
        } else if crate::services::metrics::is_calculable(&job.source_kind) {
            authorization
                .authorize_metric(&actor, job, node_id, sql)
                .await?
        } else {
            authorization.authorize(&actor, sql).await?
        };
        let masking = service.masking.clone().ok_or_else(|| {
            AppError::BadRequest("数据输出脱敏服务未配置，禁止返回业务查询数据".into())
        })?;
        let plan = match masking.prepare_output(&actor, &query).await {
            Ok(plan) => plan,
            Err(error) => {
                authorization
                    .record_access(&actor, &query, AccessOutcome::Denied)
                    .await?;
                return Err(error);
            },
        };
        Ok(Self {
            actor,
            service: Some(authorization),
            query: Some(query),
            masking: Some(masking),
            plan: Some(plan),
        })
    }

    pub async fn reauthorize(&self) -> Result<(), AppError> {
        if let (Some(service), Some(query)) = (&self.service, &self.query) {
            if let Err(error) = service.reauthorize(&self.actor, query).await {
                service
                    .record_access(&self.actor, query, AccessOutcome::Denied)
                    .await?;
                return Err(error);
            }
        }
        if let (Some(masking), Some(plan)) = (&self.masking, &self.plan) {
            if let Err(error) = masking.reauthorize_plan(&self.actor, plan).await {
                self.record(AccessOutcome::Denied).await?;
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn apply_output(
        &self,
        job: &ProductJob,
        mut result: QueryResult,
    ) -> Result<GuardedNodeOutput, AppError> {
        if let (Some(masking), Some(plan)) = (&self.masking, &self.plan) {
            if job.source_kind == "recognition_task" {
                let query = self
                    .query
                    .as_ref()
                    .ok_or_else(|| AppError::Forbidden("敏感扫描缺少真实节点查询授权".into()))?;
                // Raw sample rows exist only inside this opaque authorized scanner.
                // The worker receives counts and definition evidence, never row values.
                return crate::services::sensitive_recognition::scan(job, result, query)
                    .map(GuardedNodeOutput::Recognition);
            }
            if job.source_kind == "data_api" {
                let protected = crate::services::data_services::protect_node_result(
                    job, result, masking, plan,
                )?;
                let receipt = masking.seal_output(plan, protected.query_result())?;
                let mut mapped =
                    crate::services::data_services::map_protected_node_result(job, protected)?;
                mapped["privacy"] = serde_json::json!(receipt);
                return Ok(GuardedNodeOutput::Api(mapped));
            }
            if crate::services::metrics::is_calculable(&job.source_kind) {
                result.suggested_chart = None;
            }
            let result = masking.apply_output(plan, result)?;
            let receipt = masking.seal_output(plan, &result)?;
            Ok(GuardedNodeOutput::Query(result, Some(receipt)))
        } else {
            Ok(GuardedNodeOutput::Query(result, None))
        }
    }

    pub async fn record(&self, outcome: AccessOutcome) -> Result<(), AppError> {
        if let (Some(service), Some(query)) = (&self.service, &self.query) {
            service.record_access(&self.actor, query, outcome).await?;
        }
        Ok(())
    }
}
