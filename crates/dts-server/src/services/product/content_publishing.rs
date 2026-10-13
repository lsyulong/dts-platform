use super::ProductService;
use crate::models::{auth::AuthIdentity, error::AppError};
use dts_backend::{ProductResource, ResourceWrite, UpdateResourceParams};
use serde_json::{Value, json};

impl ProductService {
    pub(super) async fn publish_content(
        &self,
        actor: &AuthIdentity,
        record: ProductResource,
        action: &str,
    ) -> Result<Value, AppError> {
        let next = match action {
            "publish" if matches!(record.status.as_str(), "draft" | "inactive") => "published",
            "withdraw" if record.status == "published" => "inactive",
            _ => {
                return Err(AppError::Conflict(
                    "当前内容状态不支持此发布或撤回操作".into(),
                ));
            },
        };
        crate::services::product_domain::access::validate(&record.kind, &record.properties)?;
        let updated = self
            .repo
            .update(
                &actor.tenant_id,
                &record.kind,
                &record.id,
                &UpdateResourceParams {
                    expected_version: record.version,
                    actor_id: actor.sub.clone(),
                    fields: ResourceWrite {
                        code: record.code,
                        name: record.name,
                        description: record.description,
                        parent_id: record.parent_id,
                        status: next.into(),
                        properties: record.properties,
                        references: record.references,
                    },
                },
            )
            .await?;
        Ok(json!({"resource":updated,"result":{"status":next,"visibility_enforced":true}}))
    }
}
