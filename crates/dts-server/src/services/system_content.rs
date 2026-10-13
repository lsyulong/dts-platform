use std::{collections::HashSet, sync::Arc};

use chrono::Utc;
use dts_backend::{ProductResource, ProductResourceFilter, ProductResourceRepo, RoleRepo};
use serde_json::Value;

use crate::models::{auth::AuthIdentity, error::AppError};

#[derive(Clone)]
pub struct SystemContentService {
    resources: Arc<dyn ProductResourceRepo>,
    roles: Arc<dyn RoleRepo>,
    organizations: Option<Arc<dyn dts_backend::repo::OrganizationMembersRepo>>,
}

impl SystemContentService {
    pub fn new(resources: Arc<dyn ProductResourceRepo>, roles: Arc<dyn RoleRepo>) -> Self {
        Self {
            resources,
            roles,
            organizations: None,
        }
    }

    pub fn with_organizations(
        mut self,
        repo: Arc<dyn dts_backend::repo::OrganizationMembersRepo>,
    ) -> Self {
        self.organizations = Some(repo);
        self
    }

    async fn all(
        &self,
        tenant: &str,
        kind: &str,
        status: Option<&str>,
    ) -> Result<Vec<ProductResource>, AppError> {
        let mut records = Vec::new();
        loop {
            let page = self
                .resources
                .list(&ProductResourceFilter {
                    tenant_id: tenant.into(),
                    kind: kind.into(),
                    q: None,
                    status: status.map(str::to_owned),
                    limit: 100,
                    offset: records.len() as u32,
                    created_by: None,
                })
                .await?;
            let total = page.total;
            records.extend(page.data);
            if records.len() as i64 >= total {
                return Ok(records);
            }
            if records.len() >= 10_000 {
                return Err(AppError::BadRequest("已发布内容超过单次读取上限".into()));
            }
        }
    }

    pub async fn visible(
        &self,
        actor: &AuthIdentity,
        kind: &str,
    ) -> Result<Vec<ProductResource>, AppError> {
        if !matches!(kind, "announcement" | "portal") {
            return Err(AppError::NotFound("内容类型不存在".into()));
        }
        let names: HashSet<_> = self
            .roles
            .find_user_role_names(&actor.sub)
            .await?
            .into_iter()
            .collect();
        let role_ids: HashSet<_> = self
            .roles
            .list_all(Some(&actor.tenant_id))
            .await?
            .into_iter()
            .filter(|role| names.contains(&role.name))
            .map(|role| role.id)
            .collect();
        let records = self.all(&actor.tenant_id, kind, Some("published")).await?;
        let organizations: HashSet<_> = if kind == "announcement"
            && records
                .iter()
                .any(|r| r.properties["audience"]["type"].as_str() == Some("organizations"))
        {
            let repo = self
                .organizations
                .as_ref()
                .ok_or_else(|| AppError::Internal("组织成员服务未配置".into()))?;
            repo.self_organization(&actor.tenant_id, &actor.sub)
                .await?
                .into_iter()
                .collect()
        } else {
            HashSet::new()
        };
        let now = Utc::now();
        Ok(records
            .into_iter()
            .filter(|record| {
                if kind == "portal" {
                    intersects(&record.properties["authorized_role_ids"], &role_ids)
                } else {
                    announcement_visible(
                        &record.properties,
                        &actor.sub,
                        &role_ids,
                        &organizations,
                        now,
                    )
                }
            })
            .collect())
    }
}

fn contains(values: &Value, id: &str) -> bool {
    values
        .as_array()
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(id)))
}

fn intersects(values: &Value, ids: &HashSet<String>) -> bool {
    values.as_array().is_some_and(|values| {
        values
            .iter()
            .filter_map(Value::as_str)
            .any(|id| ids.contains(id))
    })
}

fn announcement_visible(
    properties: &Value,
    user: &str,
    roles: &HashSet<String>,
    organizations: &HashSet<String>,
    now: chrono::DateTime<Utc>,
) -> bool {
    for (key, future_allowed) in [("publish_at", false), ("expires_at", true)] {
        if let Some(value) = properties
            .get(key)
            .filter(|value| !value.is_null() && value.as_str() != Some(""))
        {
            let Some(time) = value
                .as_str()
                .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
            else {
                return false;
            };
            if if future_allowed {
                time <= now
            } else {
                time > now
            } {
                return false;
            }
        }
    }
    let audience = &properties["audience"];
    match audience["type"].as_str() {
        Some("tenant") => true,
        Some("users") => contains(&audience["user_ids"], user),
        Some("roles") => intersects(&audience["role_ids"], roles),
        Some("organizations") => intersects(&audience["organization_ids"], organizations),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn announcement_audience_and_validity_are_enforced() {
        let now = Utc::now();
        let roles = HashSet::from(["role_a".into()]);
        let organizations = HashSet::from(["org_a".into()]);
        assert!(announcement_visible(
            &json!({"audience":{"type":"users","user_ids":["alice"]}}),
            "alice",
            &roles,
            &organizations,
            now
        ));
        assert!(!announcement_visible(
            &json!({"audience":{"type":"users","user_ids":["bob"]}}),
            "alice",
            &roles,
            &organizations,
            now
        ));
        assert!(announcement_visible(
            &json!({"audience":{"type":"roles","role_ids":["role_a"]}}),
            "alice",
            &roles,
            &organizations,
            now
        ));
        assert!(announcement_visible(
            &json!({"audience":{"type":"organizations","organization_ids":["org_a"]}}),
            "alice",
            &roles,
            &organizations,
            now
        ));
        assert!(!announcement_visible(
            &json!({"audience":{"type":"tenant"},"expires_at":now.to_rfc3339()}),
            "alice",
            &roles,
            &organizations,
            now
        ));
        assert!(!announcement_visible(
            &json!({"audience":{"type":"tenant"},"publish_at":"invalid"}),
            "alice",
            &roles,
            &organizations,
            now
        ));
    }
}
