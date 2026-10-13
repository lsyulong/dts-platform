use super::{CallPermit, ServiceAccessService, bad, credentials::parse_token, denied};
use crate::models::{auth::AuthIdentity, error::AppError};
use crate::services::{data_access::AuthorizedQuery, data_masking::MaskingPlan};
use dts_backend::repo::data_access::{AccessOutcome, RecordDataAccessParams};
use dts_backend::repo::service_access::*;
use std::time::Instant;

impl ServiceAccessService {
    pub async fn authenticate(
        &self,
        secret: &str,
        api_id: &str,
        origin: Option<&str>,
    ) -> Result<CallPermit, AppError> {
        let started = Instant::now();
        let (id, bytes) = parse_token(secret)?;
        let credential = self
            .repo
            .lookup_credential(&id)
            .await?
            .ok_or_else(|| denied("应用凭据无效或不可用"))?;
        if !self.key.verify(
            &credential.summary.tenant_id,
            &id,
            &bytes,
            &credential.verifier,
        ) {
            return Err(denied("应用凭据无效或不可用"));
        }
        // Keep invalid headers in the authenticated denial path without storing
        // their raw contents. This value cannot occur in a valid origin allowlist.
        let origin = origin.map(|origin| {
            validation::canonical_origin(origin).unwrap_or_else(|_| "invalid-origin".into())
        });
        let record = self
            .repo
            .reserve_call(&ReserveServiceCallParams {
                call_id: uuid::Uuid::new_v4().to_string(),
                tenant_id: credential.summary.tenant_id,
                credential_id: id,
                credential_version: credential.summary.version,
                credential_verifier: credential.verifier,
                api_id: api_id.into(),
                origin,
            })
            .await?;
        match self.call_actor(&record).await {
            Ok(actor) => Ok(CallPermit {
                record,
                actor,
                started,
            }),
            Err(error) => {
                self.repo
                    .finish_call(&FinishServiceCallParams {
                        call: record,
                        outcome: ServiceCallOutcome::Denied,
                        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                        row_count: None,
                        access_audit: None,
                        masking_versions: None,
                    })
                    .await?;
                Err(error)
            },
        }
    }
    pub async fn reauthorize(&self, permit: &CallPermit) -> Result<(), AppError> {
        self.repo.reauthorize_call(&permit.record).await?;
        let actor = self.call_actor(&permit.record).await?;
        if actor != permit.actor {
            return Err(denied("调用期间应用负责人身份已变化"));
        }
        Ok(())
    }
    pub async fn finish(
        &self,
        permit: &CallPermit,
        outcome: ServiceCallOutcome,
        row_count: Option<u64>,
    ) -> Result<ServiceCallRecord, AppError> {
        let params = FinishServiceCallParams {
            call: permit.record.clone(),
            outcome,
            duration_ms: permit
                .started
                .elapsed()
                .as_millis()
                .try_into()
                .map_err(|_| bad("调用耗时超过范围"))?,
            row_count,
            access_audit: None,
            masking_versions: None,
        };
        validation::finish(&params)?;
        Ok(self.repo.finish_call(&params).await?)
    }
    pub async fn finish_with_access(
        &self,
        permit: &CallPermit,
        query: &AuthorizedQuery,
        plan: &MaskingPlan,
        row_count: u64,
    ) -> Result<ServiceCallRecord, AppError> {
        if query.actor_scope() != (permit.actor.tenant_id.as_str(), permit.actor.sub.as_str())
            || !plan.is_bound_to(&permit.actor.tenant_id, &permit.actor.sub, query.sql())
        {
            return Err(denied("访问审计及隐私计划不属于当前调用授权身份"));
        }
        let params = FinishServiceCallParams {
            call: permit.record.clone(),
            outcome: ServiceCallOutcome::Succeeded,
            duration_ms: permit
                .started
                .elapsed()
                .as_millis()
                .try_into()
                .map_err(|_| bad("调用耗时超过范围"))?,
            row_count: Some(row_count),
            access_audit: Some(RecordDataAccessParams {
                id: format!("service_call:{}", permit.record.id),
                tenant_id: permit.actor.tenant_id.clone(),
                actor_id: permit.actor.sub.clone(),
                outcome: AccessOutcome::Succeeded,
                evidence: query.evidence().clone(),
            }),
            masking_versions: Some(plan.policy_versions().to_vec()),
        };
        validation::finish(&params)?;
        Ok(self.repo.finish_call(&params).await?)
    }
    pub async fn list_calls(
        &self,
        actor: &AuthIdentity,
        application_id: Option<String>,
        api_id: Option<String>,
        limit: u32,
        offset: u32,
    ) -> Result<ServiceCallPage, AppError> {
        self.authority(actor).await?;
        validation::pagination(limit, offset)?;
        Ok(self
            .repo
            .list_calls(&ServiceCallFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                application_id,
                api_id,
                limit,
                offset,
            })
            .await?)
    }
    async fn call_actor(&self, call: &ServiceCallRecord) -> Result<AuthIdentity, AppError> {
        let detail = self
            .users
            .find_detail_by_id(&call.owner_id)
            .await?
            .filter(|user| user.tenant_id == call.tenant_id)
            .ok_or_else(|| denied("应用负责人不属于实际租户"))?;
        let user = self
            .users
            .find_by_tenant_and_username(&call.tenant_id, &detail.username)
            .await?
            .filter(|user| {
                user.id == call.owner_id
                    && user.tenant_id == call.tenant_id
                    && user.status == "active"
            })
            .ok_or_else(|| denied("应用负责人已停用或不存在"))?;
        Ok(AuthIdentity {
            sub: user.id,
            tenant_id: user.tenant_id,
            username: user.username,
        })
    }
}
