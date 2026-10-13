use dts_backend::{CreateResourceParams, ResourceWrite};
use serde_json::json;

use super::ProductService;
use crate::models::error::AppError;

impl ProductService {
    pub async fn record_login(
        &self,
        tenant_id: &str,
        user_id: Option<&str>,
        username: &str,
        source_ip: &str,
        succeeded: bool,
    ) -> Result<(), AppError> {
        self.repo.create(&CreateResourceParams {
            id:uuid::Uuid::new_v4().to_string(),tenant_id:tenant_id.into(),kind:"login_event".into(),actor_id:user_id.unwrap_or("unauthenticated").into(),
            fields:ResourceWrite {
                code:format!("login_{}",uuid::Uuid::new_v4().simple()),name:username.into(),description:None,parent_id:None,
                status:if succeeded {"succeeded"} else {"failed"}.into(),
                properties:json!({"user_id":user_id,"username":username,"source_ip":source_ip,"outcome":if succeeded {"success"} else {"failure"},"failure_reason":if succeeded {None} else {Some("authentication_rejected")}}),
                references:vec![],
            },
        }).await?;
        Ok(())
    }
}
