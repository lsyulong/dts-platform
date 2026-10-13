use dts_backend::ProductResource;
use serde_json::{Value, json};

use super::ProductService;
use crate::models::{auth::AuthIdentity, error::AppError};

impl ProductService {
    pub(super) async fn publish_model(
        &self,
        actor: &AuthIdentity,
        record: ProductResource,
        expected_version: i64,
    ) -> Result<Value, AppError> {
        match self.analytical_tenant.as_deref() {
            Some(tenant) if tenant == actor.tenant_id => {},
            Some(_) => return Err(AppError::Forbidden("当前租户没有模型发布引擎".into())),
            None => return Err(AppError::BadRequest("SwanLake 模型发布引擎未启用".into())),
        }
        use crate::services::standards::StandardModelEngineOutcome;
        let standards = self
            .standards
            .as_ref()
            .ok_or_else(|| AppError::Internal("模型发布门禁未配置".into()))?;
        let claim = standards
            .prepare_model_publication(actor, &record.id, expected_version)
            .await?;
        let result = match standards.reauthorize_model_publication(actor, &claim).await {
            Err(error) => Err(error),
            Ok(()) if claim.needs_reconciliation() => {
                self.publisher.reconcile(&claim.resource().properties).await
            },
            Ok(()) => self.publisher.publish(&claim.resource().properties).await,
        };
        let outcome = match &result {
            Ok(receipt) => StandardModelEngineOutcome::Published {
                receipt: receipt.clone(),
            },
            Err(error)
                if claim.needs_reconciliation() || matches!(error, AppError::Conflict(_)) =>
            {
                StandardModelEngineOutcome::Unknown {
                    reason: error.to_string(),
                    receipt: None,
                }
            },
            Err(error) => StandardModelEngineOutcome::Failed {
                reason: error.to_string(),
            },
        };
        let completed = standards
            .finish_model_publication(actor, &claim, outcome)
            .await?;
        match result {
            Ok(receipt)
                if completed.resource.status == "published"
                    && completed.publication.status == "published" =>
            {
                Ok(
                    json!({"resource":completed.resource,"result":receipt,"publication":completed.publication,"standards_state":completed.standards_state}),
                )
            },
            Ok(_) => Err(AppError::Conflict(
                "已记录真实物理结果，但发布权限、标准或模型版本已变化，结果待核对".into(),
            )),
            Err(error) => Err(error),
        }
    }
}
