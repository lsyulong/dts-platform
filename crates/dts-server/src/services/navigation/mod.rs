use crate::models::{auth::AuthIdentity, error::AppError};
use chrono::{DateTime, Utc};
use dts_backend::repo::navigation::*;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignNavigationRequest {
    pub expected_template_version: i64,
    pub expected_assignment_version: i64,
    pub idempotency_key: String,
    pub expires_at: Option<DateTime<Utc>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeNavigationRequest {
    pub assignment_id: String,
    pub expected_assignment_version: i64,
    pub idempotency_key: String,
    pub reason: String,
}
#[derive(Clone)]
pub struct NavigationService {
    repo: Arc<dyn NavigationRepo>,
    registry: Arc<NavigationRegistry>,
}
impl NavigationService {
    pub fn new(repo: Arc<dyn NavigationRepo>, registry: Arc<NavigationRegistry>) -> Self {
        Self { repo, registry }
    }
    pub async fn current(&self, actor: &AuthIdentity) -> Result<NavigationView, AppError> {
        Ok(self
            .repo
            .current(&ReadNavigationParams {
                scope: scope(actor),
                registry: self.registry.clone(),
            })
            .await?)
    }
    pub async fn assign(
        &self,
        actor: &AuthIdentity,
        template_id: &str,
        request: AssignNavigationRequest,
    ) -> Result<NavigationDecisionResult, AppError> {
        Ok(self
            .repo
            .assign(&AssignNavigationParams {
                scope: scope(actor),
                template_id: template_id.into(),
                expected_template_version: request.expected_template_version,
                expected_assignment_version: request.expected_assignment_version,
                idempotency_key: request.idempotency_key,
                expires_at: request.expires_at,
                registry: self.registry.clone(),
            })
            .await?)
    }
    pub async fn revoke(
        &self,
        actor: &AuthIdentity,
        request: RevokeNavigationRequest,
    ) -> Result<NavigationDecisionResult, AppError> {
        Ok(self
            .repo
            .revoke(&RevokeNavigationParams {
                scope: scope(actor),
                assignment_id: request.assignment_id,
                expected_assignment_version: request.expected_assignment_version,
                idempotency_key: request.idempotency_key,
                reason: request.reason,
            })
            .await?)
    }
    pub async fn history(
        &self,
        actor: &AuthIdentity,
        limit: u32,
        offset: u32,
    ) -> Result<NavigationHistoryPage, AppError> {
        Ok(self
            .repo
            .history(&NavigationHistoryFilter {
                scope: scope(actor),
                limit,
                offset,
            })
            .await?)
    }
}
fn scope(actor: &AuthIdentity) -> NavigationScope {
    NavigationScope {
        tenant_id: actor.tenant_id.clone(),
        actor_id: actor.sub.clone(),
    }
}

pub fn product_registry() -> Result<Arc<NavigationRegistry>, AppError> {
    let routes = crate::services::product::catalog::modules()?
        .iter()
        .enumerate()
        .map(|(index, module)| {
            let permission = if matches!(
                module.resource_kind.as_deref(),
                Some(
                    "user_profile"
                        | "work_item"
                        | "usage_statistics"
                        | "system_message"
                        | "announcement"
                        | "portal"
                )
            ) {
                None
            } else {
                module.permission.clone().or_else(|| {
                    module.resource_kind.as_ref().map(|_| {
                        format!("{}:read", crate::services::product::catalog::scope(module))
                    })
                })
            };
            RegisteredNavigationRoute {
                id: module.id.clone(),
                path: module.path.clone(),
                name: module.title.zh.clone(),
                required_permissions: permission.into_iter().collect(),
                sort_order: index as u32,
            }
        })
        .collect();
    Ok(Arc::new(NavigationRegistry::new(routes)?))
}

#[cfg(test)]
mod tests {
    #[test]
    fn actual_product_catalog_builds_a_complete_registry_with_legacy_permissions() {
        let registry = super::product_registry().expect("valid actual registered catalog");
        assert_eq!(
            registry.routes().len(),
            crate::services::product::catalog::modules().unwrap().len()
        );
        assert_eq!(
            registry
                .route("/system/users")
                .unwrap()
                .required_permissions,
            vec!["user:manage"]
        );
        assert!(
            registry
                .route("/account/profile")
                .unwrap()
                .required_permissions
                .is_empty()
        );
        assert_eq!(
            registry.route("/tasks").unwrap().required_permissions,
            vec!["task:read"]
        );
    }
}
