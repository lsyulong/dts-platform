use crate::services::data_access::AuthorizedQuery;
use dts_backend::repo::data_access::DataAccessTarget;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuleDefinition {
    pub object_id: String,
    pub column_name: String,
    pub method: String,
    #[serde(default)]
    pub parameters: Value,
    pub metadata_version: Option<i64>,
    pub column_data_type: Option<String>,
    pub source_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GroupDefinition {
    pub member_ids: Vec<String>,
    pub masking_rule_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct BoundRule {
    pub rule_id: String,
    pub rule_version: i64,
    pub groups: Vec<(String, i64)>,
    pub definition: RuleDefinition,
    pub target: DataAccessTarget,
}

#[derive(Clone)]
pub struct MaskingPlan {
    pub(super) query: AuthorizedQuery,
    pub(super) policy_hash: String,
    pub(super) fields: Vec<OutputMask>,
    pub(super) policy_versions: Vec<dts_backend::repo::data_access::PrivacyPolicyVersion>,
}

impl MaskingPlan {
    pub(crate) fn policy_versions(
        &self,
    ) -> &[dts_backend::repo::data_access::PrivacyPolicyVersion] {
        &self.policy_versions
    }
    pub(crate) fn is_bound_to(&self, tenant_id: &str, actor_id: &str, sql: &str) -> bool {
        self.query.actor_scope() == (tenant_id, actor_id) && self.query.sql() == sql
    }
    pub fn output_columns(&self) -> Vec<MaskingOutputDescriptor> {
        self.fields
            .iter()
            .map(|field| MaskingOutputDescriptor {
                name: field.output_name.clone(),
                data_type: "VARCHAR".into(),
                may_introduce_null: false,
            })
            .collect()
    }
    pub fn policy_hash(&self) -> &str {
        &self.policy_hash
    }
    pub fn has_masks(&self) -> bool {
        !self.fields.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskingOutputDescriptor {
    pub name: String,
    pub data_type: String,
    pub may_introduce_null: bool,
}

#[derive(Clone)]
pub(super) struct OutputMask {
    pub output_name: String,
    pub rule: BoundRule,
}

/// This is a server-signed description of a complete stored query result.
/// A client-supplied flag is never sufficient to skip masking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaskingReceipt {
    pub version: u8,
    pub tenant_id: String,
    pub query_hash: String,
    pub policy_hash: String,
    pub result_hash: String,
    pub signature: String,
}
