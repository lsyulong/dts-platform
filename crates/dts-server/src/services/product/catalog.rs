use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::models::error::AppError;

#[derive(Clone, Deserialize, Serialize)]
pub struct ProductText {
    pub zh: String,
    pub en: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct FieldOption {
    pub value: String,
    pub label: ProductText,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ResourceField {
    pub key: String,
    pub label: ProductText,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, rename = "readOnly")]
    pub read_only: bool,
    #[serde(rename = "referenceKind", skip_serializing_if = "Option::is_none")]
    pub reference_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<FieldOption>,
}

#[derive(Clone, Deserialize)]
pub struct ProductModule {
    pub id: String,
    pub path: String,
    pub title: ProductText,
    pub permission: Option<String>,
    #[serde(rename = "groupId")]
    pub group_id: String,
    #[serde(rename = "resourceKind")]
    pub resource_kind: Option<String>,
    #[serde(default)]
    pub fields: Vec<ResourceField>,
    #[serde(default)]
    pub actions: Vec<String>,
}

#[derive(Deserialize)]
struct Catalog {
    modules: Vec<ProductModule>,
}

static CATALOG: LazyLock<Result<Catalog, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../resources/product/catalog.json"))
        .map_err(|e| e.to_string())
});

pub fn modules() -> Result<&'static [ProductModule], AppError> {
    CATALOG
        .as_ref()
        .map(|catalog| catalog.modules.as_slice())
        .map_err(|_| AppError::Internal("产品领域目录无效".into()))
}

pub fn module(kind: &str) -> Result<&'static ProductModule, AppError> {
    modules()?
        .iter()
        .find(|module| module.resource_kind.as_deref() == Some(kind))
        .ok_or_else(|| AppError::NotFound("业务模块不存在".into()))
}

pub fn scope(module: &ProductModule) -> String {
    if module.group_id == "access" {
        "system".into()
    } else {
        module.group_id.replace('-', "_")
    }
}

pub fn read_only(kind: &str) -> bool {
    matches!(
        kind,
        "user_profile"
            | "work_item"
            | "system_statistics"
            | "usage_statistics"
            | "login_event"
            | "operation_event"
            | "workflow_run"
            | "metadata_object"
            | "sensitive_finding"
            | "quality_result"
            | "quality_report"
            | "api_call"
            | "asset_analysis"
            | "data_access_audit"
            | "data_access_approval"
    )
}

pub fn hierarchical(kind: &str) -> bool {
    matches!(
        kind,
        "organization"
            | "system_menu"
            | "warehouse_category"
            | "asset_directory"
            | "development_folder"
    )
}

#[derive(Serialize)]
pub struct ResourceSchema {
    pub kind: String,
    pub fields: Vec<ResourceField>,
    pub read_permission: String,
    pub write_permission: String,
    pub statuses: Vec<&'static str>,
    pub actions: Vec<&'static str>,
}

pub fn schema(module: &ProductModule) -> ResourceSchema {
    let kind = module.resource_kind.as_deref().unwrap_or_default();
    let domain = scope(module);
    let mut actions: Vec<&'static str> = if read_only(kind) {
        vec![]
    } else {
        ["create", "update", "delete"]
            .into_iter()
            .filter(|action| module.actions.iter().any(|a| a == action))
            .collect()
    };
    if matches!(
        kind,
        "naming_standard"
            | "data_model"
            | "field_standard"
            | "enum_standard"
            | "unit_standard"
            | "metric_period"
            | "atomic_metric"
            | "derived_metric"
            | "composite_metric"
    ) {
        actions.push("validate");
    }
    if kind == "unit_standard" {
        actions.push("convert");
    }
    if dts_backend::repo::standard_governance::STANDARD_KINDS.contains(&kind) {
        actions.extend(["publish", "revise", "withdraw"]);
    }
    if crate::services::metrics::handles(kind) {
        actions.extend(["validate", "publish", "withdraw"]);
        if crate::services::metrics::is_calculable(kind) {
            actions.push("calculate");
        }
    }
    if matches!(
        kind,
        "data_classification" | "recognition_rule" | "recognition_task"
    ) {
        actions.extend(["validate", "publish", "withdraw"]);
        if kind == "recognition_task" {
            actions.extend(["execute", "cancel"]);
        }
    }
    if kind == "data_model" {
        actions.push("publish");
    }
    if kind == "system_message" {
        actions.push("send");
    }
    if kind == "message_template" {
        actions.extend(["validate", "preview"]);
    }
    if kind == "portal" {
        actions.push("preview");
    }
    if matches!(kind, "portal" | "announcement") {
        actions.extend(["publish", "withdraw"]);
    }
    if matches!(kind, "sql_document" | "development_workflow") {
        actions.extend(["save_version", "validate", "publish", "withdraw", "execute"]);
    }
    if kind == "development_workflow" {
        actions.extend(["register_schedule", "unschedule", "cancel"]);
    }
    if kind == "development_config" {
        actions.extend(["validate", "publish", "withdraw"]);
    }
    if matches!(kind, "master_data_model" | "master_data_record") {
        actions.push("validate");
    }
    if kind == "master_data_model" {
        actions.push("publish");
    }
    if kind == "master_code_rule" {
        actions.push("allocate");
    }
    if kind == "master_data_record" {
        actions.extend(["submit", "approve", "reject"]);
    }
    if matches!(kind, "quality_object" | "quality_rule" | "quality_task") {
        actions.push("validate");
    }
    if kind == "quality_task" {
        actions.push("execute");
    }
    if kind == "quality_monitor" {
        actions.push("evaluate");
    }
    if kind == "metadata_collection" {
        actions.extend(["validate", "execute"]);
    }
    if kind == "bi_report" {
        actions.extend(["validate", "publish", "execute"]);
    }
    if kind == "data_api" {
        actions.extend(["validate", "publish", "withdraw", "execute"]);
    }
    if kind == "asset_entry" {
        actions.extend(["register", "publish", "withdraw"]);
    }
    if kind == "asset_map" {
        actions.push("analyze");
    }
    if kind == "metadata_lineage" {
        actions.push("analyze");
    }
    if matches!(kind, "masking_rule" | "user_group") {
        actions.extend(["validate", "publish", "withdraw"]);
    }
    if kind == "masking_rule" {
        actions.push("preview");
    }
    if kind == "user_group" {
        actions.push("assign_members");
    }
    if kind == "tag" {
        actions.extend(["assign", "unassign"]);
    }
    if kind == "alert_policy" {
        actions.extend(["evaluate", "test_delivery"]);
    }
    if kind == "service_application" {
        actions.retain(|action| *action != "delete");
        actions.push("archive");
    }
    if kind == "service_subscription" {
        actions.retain(|action| *action != "delete");
        actions.extend(["update", "submit", "approve", "reject", "revoke"]);
    }
    if kind == "data_access_request" {
        actions.extend(["submit", "withdraw"]);
    }
    actions.sort_unstable();
    actions.dedup();
    let statuses = match kind {
        "sensitive_finding" => vec!["proposed", "confirmed", "dismissed"],
        "service_application" => vec!["active", "inactive"],
        "service_subscription" => vec!["draft", "pending", "approved", "rejected", "revoked"],
        "asset_entry" => vec!["draft", "registered", "published", "withdrawn"],
        "master_data_record" => vec!["draft", "pending", "approved", "rejected"],
        "system_message" => vec!["draft", "sent", "failed"],
        "metadata_object" => vec!["published", "missing"],
        "login_event" => vec!["succeeded", "failed"],
        "data_access_request" | "data_access_approval" => vec![
            "draft",
            "pending",
            "approved",
            "rejected",
            "withdrawn",
            "revoked",
        ],
        _ if read_only(kind) => vec![],
        _ => vec![
            "draft",
            "publishing",
            "published",
            "publish_failed",
            "publish_unknown",
            "inactive",
        ],
    };
    ResourceSchema {
        kind: kind.into(),
        fields: module.fields.clone(),
        read_permission: format!("{domain}:read"),
        write_permission: if read_only(kind) {
            String::new()
        } else {
            format!("{domain}:write")
        },
        statuses,
        actions,
    }
}
