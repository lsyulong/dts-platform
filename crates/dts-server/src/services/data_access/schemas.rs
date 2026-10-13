use super::{QueryAuthorizationService, denied};
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::query_svc::{SchemaInfo, TableInfo},
};
use chrono::Utc;
use std::collections::{BTreeMap, BTreeSet};

impl QueryAuthorizationService {
    pub async fn has_all_data(&self, actor: &AuthIdentity) -> Result<bool, AppError> {
        if self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str()) {
            return Err(denied("当前租户未绑定受控查询引擎"));
        }
        let authority = self
            .repo
            .actor_authority(&actor.tenant_id, &actor.sub)
            .await?;
        if !authority.active {
            return Err(denied("当前用户不存在或已停用"));
        }
        Ok(authority.all_data)
    }

    /// Grant-backed schema names only. The caller can use has_all_data before
    /// and after the separate engine's schema endpoint for explicit admins.
    pub async fn visible_schemas(&self, actor: &AuthIdentity) -> Result<Vec<SchemaInfo>, AppError> {
        self.has_all_data(actor).await?;
        let now = Utc::now();
        let grants = self
            .repo
            .active_grants(&actor.tenant_id, &actor.sub, now)
            .await?;
        if grants.len() > 1000 {
            return Err(denied("有效授权超过目录容量"));
        }
        let mut schemas: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for grant in grants {
            if grant.tenant_id != actor.tenant_id
                || grant.grantee_id != actor.sub
                || grant.revoked_at.is_some()
                || grant.expires_at <= now
            {
                continue;
            }
            let target = &grant.target;
            let binding = self
                .metadata
                .verified_binding(
                    &actor.tenant_id,
                    &target.metadata_id,
                    target.binding.object_version,
                )
                .await?;
            if binding.as_ref() != Some(&target.binding) {
                continue;
            }
            schemas
                .entry((
                    target.binding.table_catalog.clone(),
                    target.binding.schema_name.clone(),
                ))
                .or_default()
                .insert(target.binding.table_name.clone());
        }
        if !self
            .repo
            .actor_authority(&actor.tenant_id, &actor.sub)
            .await?
            .active
        {
            return Err(denied("当前用户已停用"));
        }
        Ok(schemas
            .into_iter()
            .map(|((catalog, schema), tables)| SchemaInfo {
                catalog: Some(catalog),
                schema,
                tables: tables.into_iter().map(|name| TableInfo { name }).collect(),
            })
            .collect())
    }
}
