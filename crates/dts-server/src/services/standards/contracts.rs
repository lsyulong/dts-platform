use dts_backend::repo::standard_governance::StandardCoverage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandardVersionRequest {
    pub id: String,
    pub version: i64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnStandardRequest {
    pub column_name: String,
    pub field_standard: Option<StandardVersionRequest>,
    pub naming_standard: Option<StandardVersionRequest>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelStandardsRequest {
    pub expected_model_version: i64,
    pub expected_binding_version: Option<i64>,
    pub coverage: StandardCoverage,
    pub columns: Vec<ColumnStandardRequest>,
    pub table_naming: Option<StandardVersionRequest>,
    #[serde(default)]
    pub upgrade: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WithdrawStandardRequest {
    pub expected_version: i64,
    pub revision_version: i64,
    pub expected_state_version: i64,
    pub reason: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectModelStandardsRequest {
    pub expected_model_version: i64,
    pub expected_binding_version: i64,
    pub metadata_id: String,
    pub metadata_version: i64,
}
#[derive(Debug, Serialize)]
pub struct StandardValidation {
    pub valid: bool,
    pub validation_scope: &'static str,
    pub executed: bool,
    pub dependency_versions: Vec<dts_backend::repo::standard_governance::FrozenStandardRef>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandardRevisionsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
