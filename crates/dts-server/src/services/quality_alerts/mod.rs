use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::{auth_svc::AuthService, data_access::QueryAuthorizationService},
};
use dts_backend::{
    ProductResourceRepo,
    repo::{metadata_catalog::MetadataCatalogRepo, product_job::ProductJobRepo, quality_alerts::*},
};
use serde_json::Value;
use std::sync::Arc;

mod authorization;
mod collect;
mod definition;
#[cfg(test)]
mod tests;
mod types;
mod views;
pub use definition::validate_definition;
pub use types::*;

#[derive(Clone)]
pub struct QualityAlertsService {
    resources: Arc<dyn ProductResourceRepo>,
    jobs: Arc<dyn ProductJobRepo>,
    metadata: Arc<dyn MetadataCatalogRepo>,
    guard: Arc<QueryAuthorizationService>,
    auth: AuthService,
    alerts: Arc<dyn QualityAlertRepo>,
}
impl QualityAlertsService {
    pub fn new(
        resources: Arc<dyn ProductResourceRepo>,
        jobs: Arc<dyn ProductJobRepo>,
        metadata: Arc<dyn MetadataCatalogRepo>,
        guard: Arc<QueryAuthorizationService>,
        auth: AuthService,
        alerts: Arc<dyn QualityAlertRepo>,
    ) -> Self {
        Self {
            resources,
            jobs,
            metadata,
            guard,
            auth,
            alerts,
        }
    }
    async fn permission(&self, actor: &AuthIdentity, write: bool) -> Result<(), AppError> {
        self.auth.validate_identity(actor).await?;
        self.auth.authorize(actor, "quality:read").await?;
        if write {
            self.auth.authorize(actor, "quality:write").await?;
        }
        Ok(())
    }
    pub async fn evaluate(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: QualityEvaluateRequest,
    ) -> Result<QualityEvaluationView, AppError> {
        self.permission(actor, true).await?;
        crate::services::product_jobs::validate_idempotency_key(&request.idempotency_key)?;
        if let Some(replay) = self
            .alerts
            .replay_evaluation(
                &actor.tenant_id,
                &actor.sub,
                id,
                request.expected_version,
                &request.idempotency_key,
            )
            .await?
        {
            return self.evaluation_view(actor, replay).await;
        }
        let params = self.collect(actor, id, request).await?;
        // These are real read entitlements; no synthetic query is executed.
        let tokens = self
            .authorize_observation(actor, &params.observation)
            .await?;
        self.finish_read(actor, &tokens).await?;
        let result = self.alerts.record_evaluation(&params).await?;
        self.evaluation_view(actor, result).await
    }
    pub async fn list_observations(
        &self,
        actor: &AuthIdentity,
        monitor_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<QualityObservationPageView, AppError> {
        self.permission(actor, false).await?;
        let filter = QualityAlertFilter {
            tenant_id: actor.tenant_id.clone(),
            monitor_id: monitor_id.into(),
            status: None,
            limit,
            offset,
        };
        let page = self.alerts.list_observations(&filter).await?;
        self.authorize_monitor(actor, monitor_id).await?;
        let mut data = Vec::new();
        for observation in page.data {
            data.push(self.observation_view(actor, observation).await?);
        }
        self.permission(actor, false).await?;
        Ok(QualityPage {
            data,
            total: page.total,
        })
    }
    pub async fn list_alerts(
        &self,
        actor: &AuthIdentity,
        monitor_id: &str,
        status: Option<String>,
        limit: u32,
        offset: u32,
    ) -> Result<QualityAlertPageView, AppError> {
        self.permission(actor, false).await?;
        let page = self
            .alerts
            .list_alerts(&QualityAlertFilter {
                tenant_id: actor.tenant_id.clone(),
                monitor_id: monitor_id.into(),
                status,
                limit,
                offset,
            })
            .await?;
        self.authorize_monitor(actor, monitor_id).await?;
        let mut data = Vec::new();
        for alert in page.data {
            data.push(self.alert_view(actor, alert).await?);
        }
        self.permission(actor, false).await?;
        Ok(QualityPage {
            data,
            total: page.total,
        })
    }
    pub async fn acknowledge(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: QualityAckRequest,
    ) -> Result<QualityAlertView, AppError> {
        self.permission(actor, true).await?;
        crate::services::product_jobs::validate_idempotency_key(&request.idempotency_key)?;
        let alert = self
            .alerts
            .get_alert(&actor.tenant_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound("质量告警不存在".into()))?;
        self.authorize_monitor(actor, &alert.monitor_id).await?;
        let result = self
            .alerts
            .acknowledge(&AcknowledgeQualityAlertParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                alert_id: id.into(),
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reason: request.reason,
            })
            .await?;
        self.alert_view(actor, result).await
    }
    pub async fn list_events(
        &self,
        actor: &AuthIdentity,
        id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<QualityAlertEventPage, AppError> {
        self.permission(actor, false).await?;
        let alert = self
            .alerts
            .get_alert(&actor.tenant_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound("质量告警不存在".into()))?;
        self.alert_view(actor, alert).await?;
        let page = self
            .alerts
            .list_events(&actor.tenant_id, id, limit, offset)
            .await?;
        // Events can refer to older observations; every linked observation is
        // independently checked before its outcome/reason is released.
        for event in &page.data {
            let observation = self
                .alerts
                .get_observation(&actor.tenant_id, &event.observation_id)
                .await?
                .ok_or_else(|| AppError::Internal("告警事件缺少真实观察记录".into()))?;
            self.observation_view(actor, observation).await?;
        }
        self.permission(actor, false).await?;
        Ok(page)
    }
    pub async fn list_notices(
        &self,
        actor: &AuthIdentity,
        unread_only: bool,
        limit: u32,
        offset: u32,
    ) -> Result<QualityPage<QualityAlertNotice>, AppError> {
        self.auth.validate_identity(actor).await?;
        let page = self
            .alerts
            .list_notices(&actor.tenant_id, &actor.sub, unread_only, limit, offset)
            .await?;
        self.auth.validate_identity(actor).await?;
        Ok(page)
    }
    pub async fn mark_notice_read(
        &self,
        actor: &AuthIdentity,
        id: &str,
    ) -> Result<QualityAlertNotice, AppError> {
        self.auth.validate_identity(actor).await?;
        let notice = self
            .alerts
            .mark_notice_read(&actor.tenant_id, &actor.sub, id)
            .await?;
        self.auth.validate_identity(actor).await?;
        Ok(notice)
    }
    async fn resource(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
    ) -> Result<dts_backend::ProductResource, AppError> {
        self.resources
            .get(&actor.tenant_id, kind, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("{kind} 不存在或不属于当前租户")))
    }
}
pub(crate) fn text(value: &Value, key: &str) -> Result<String, AppError> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| AppError::BadRequest(format!("{key} 必须是真实资源标识")))
}
