use super::{DataAccessService, denied};
use crate::models::{auth::AuthIdentity, error::AppError};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EffectiveDataGrant {
    pub id: String,
    pub request_id: String,
    pub metadata_id: String,
    pub metadata_version: i64,
    pub table_catalog: String,
    pub schema_name: String,
    pub table_name: String,
    pub columns: Vec<String>,
    pub expires_at: DateTime<Utc>,
}

impl DataAccessService {
    /// Exposes only authorized columns, never the complete frozen schema.
    pub async fn list_active_grants(
        &self,
        actor: &AuthIdentity,
    ) -> Result<Vec<EffectiveDataGrant>, AppError> {
        self.active(actor).await?;
        if self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str()) {
            return Err(denied("当前租户未绑定受控引擎"));
        }
        let now = Utc::now();
        let grants = self
            .repo
            .active_grants(&actor.tenant_id, &actor.sub, now)
            .await?;
        if grants.len() > 1000 {
            return Err(denied("有效授权超过目录容量"));
        }
        let mut result = Vec::new();
        for grant in grants {
            if grant.tenant_id != actor.tenant_id
                || grant.grantee_id != actor.sub
                || grant.expires_at <= now
                || grant.revoked_at.is_some()
            {
                continue;
            }
            let target = grant.target;
            if self
                .metadata
                .verified_binding(
                    &actor.tenant_id,
                    &target.metadata_id,
                    target.binding.object_version,
                )
                .await?
                .as_ref()
                != Some(&target.binding)
            {
                continue;
            }
            result.push(EffectiveDataGrant {
                id: grant.id,
                request_id: grant.request_id,
                metadata_id: target.metadata_id,
                metadata_version: target.binding.object_version,
                table_catalog: target.binding.table_catalog,
                schema_name: target.binding.schema_name,
                table_name: target.binding.table_name,
                columns: grant.columns,
                expires_at: grant.expires_at,
            });
        }
        Ok(result)
    }
}
