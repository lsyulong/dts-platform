use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RegisteredNavigationRoute {
    pub id: String,
    pub path: String,
    pub name: String,
    pub required_permissions: Vec<String>,
    pub sort_order: u32,
}
/// Server-owned canonical catalog. It cannot be deserialized from an HTTP body.
#[derive(Debug, Clone)]
pub struct NavigationRegistry {
    routes: Vec<RegisteredNavigationRoute>,
}
impl NavigationRegistry {
    pub fn new(routes: Vec<RegisteredNavigationRoute>) -> Result<Self, crate::BackendError> {
        super::validation::registry(&routes)?;
        Ok(Self { routes })
    }
    pub fn routes(&self) -> &[RegisteredNavigationRoute] {
        &self.routes
    }
    pub fn route(&self, path: &str) -> Option<&RegisteredNavigationRoute> {
        self.routes.iter().find(|route| route.path == path)
    }
}
#[derive(Debug, Clone)]
pub struct NavigationScope {
    pub tenant_id: String,
    pub actor_id: String,
}
pub struct AssignNavigationParams {
    pub scope: NavigationScope,
    pub template_id: String,
    pub expected_template_version: i64,
    pub expected_assignment_version: i64,
    pub idempotency_key: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub registry: Arc<NavigationRegistry>,
}
pub struct RevokeNavigationParams {
    pub scope: NavigationScope,
    pub assignment_id: String,
    pub expected_assignment_version: i64,
    pub idempotency_key: String,
    pub reason: String,
}
pub struct ReadNavigationParams {
    pub scope: NavigationScope,
    pub registry: Arc<NavigationRegistry>,
}
pub struct NavigationHistoryFilter {
    pub scope: NavigationScope,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenNavigationMenu {
    pub id: String,
    pub version: i64,
    pub parent_id: Option<String>,
    pub name: String,
    pub path: String,
    pub additional_permission: Option<String>,
    pub visible: bool,
    pub sort_order: u32,
    pub icon: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NavigationTemplateSnapshot {
    pub id: String,
    pub version: i64,
    pub name: String,
    pub edition: Option<String>,
    pub validity_days: Option<u32>,
    pub menus: Vec<FrozenNavigationMenu>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NavigationAssignmentStatus {
    Active,
    Revoked,
}
impl NavigationAssignmentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NavigationAssignment {
    pub id: String,
    pub tenant_id: String,
    pub version: i64,
    pub status: NavigationAssignmentStatus,
    pub template: NavigationTemplateSnapshot,
    pub assigned_by: String,
    pub assigned_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_by: Option<String>,
    pub revoked_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationDecisionResult {
    pub assignment: NavigationAssignment,
    pub receipt_id: String,
    pub replayed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NavigationMode {
    DefaultCatalog,
    Assigned,
    Expired,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationNode {
    pub id: String,
    pub menu_version: Option<i64>,
    pub parent_id: Option<String>,
    pub name: String,
    pub path: String,
    pub sort_order: u32,
    pub icon: Option<String>,
    pub source: String,
    pub recovery: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationView {
    pub tenant_id: String,
    pub mode: NavigationMode,
    pub assignment_id: Option<String>,
    pub assignment_version: i64,
    pub template_id: Option<String>,
    pub template_version: Option<i64>,
    pub expires_at: Option<DateTime<Utc>>,
    pub nodes: Vec<NavigationNode>,
    pub filtered_count: u32,
    pub observed_at: DateTime<Utc>,
    pub affects_navigation_only: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationHistoryEntry {
    pub id: String,
    pub assignment_id: String,
    pub action: String,
    pub actor_id: String,
    pub assignment_version: i64,
    pub result: NavigationAssignment,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Serialize)]
pub struct NavigationHistoryPage {
    pub data: Vec<NavigationHistoryEntry>,
    pub total: i64,
}
