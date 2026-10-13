use crate::{BackendError, ResourceWrite};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const MAX_LINEAGE_EDGES: u32 = 2000;
pub struct LineageSnapshotParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub max_edges: u32,
    pub root_object_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageNode {
    pub id: String,
    pub code: String,
    pub name: String,
    pub version: i64,
    pub status: String,
    pub table_catalog: String,
    pub schema_name: String,
    pub table_name: String,
    pub source_fingerprint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageEdge {
    pub id: String,
    pub version: i64,
    pub source_object_id: String,
    pub target_object_id: String,
    pub source_object_version: i64,
    pub target_object_version: i64,
    pub origin: String,
    pub evidence: String,
    pub evidence_actor_id: String,
    pub evidence_recorded_at: DateTime<Utc>,
    pub stale: bool,
    pub invalid_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LineageSnapshot {
    pub nodes: Vec<LineageNode>,
    pub edges: Vec<LineageEdge>,
    pub invalid_edges: Vec<InvalidLineageEdge>,
    pub total_edges: i64,
    pub scanned_edges: usize,
    pub scan_truncated: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct InvalidLineageEdge {
    pub id: String,
    pub version: i64,
    pub reason: String,
}
#[async_trait]
pub trait MetadataLineageRepo: Send + Sync {
    async fn snapshot(
        &self,
        params: &LineageSnapshotParams,
    ) -> Result<LineageSnapshot, BackendError>;
}

pub fn decode_edge(
    id: &str,
    version: i64,
    fields: &ResourceWrite,
) -> Result<LineageEdge, BackendError> {
    let properties = &fields.properties;
    let text = |key: &str, max: usize| -> Result<String, BackendError> {
        properties[key]
            .as_str()
            .filter(|value| !value.trim().is_empty() && value.len() <= max && !value.contains('\0'))
            .map(str::to_owned)
            .ok_or_else(|| invalid("血缘标识或人工证据为空或无效"))
    };
    if properties["origin"] != "manual" {
        return Err(invalid(
            "SQL/采集器血缘必须由真实受控解析器提交，不能由人工冒充",
        ));
    }
    let source = text("source_object_id", 128)?;
    let target = text("target_object_id", 128)?;
    let actor = text("evidence_actor_id", 128)?;
    for (field, kind, target_id) in [
        ("source_object_id", "metadata_object", &source),
        ("target_object_id", "metadata_object", &target),
        ("evidence_actor_id", "user", &actor),
    ] {
        if !fields.references.iter().any(|reference| {
            reference.field == field
                && reference.target_kind == kind
                && &reference.target_id == target_id
        }) {
            return Err(invalid("血缘属性与持久化的真实引用不一致"));
        }
    }
    for value in [&source, &target, &actor] {
        if value.chars().any(char::is_control) {
            return Err(invalid("血缘标识包含控制字符"));
        }
    }
    let version_value = |key: &str| {
        properties[key]
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or_else(|| invalid("血缘缺少真实端点版本"))
    };
    let stamp = text("evidence_recorded_at", 64)?;
    let evidence_recorded_at = DateTime::parse_from_rfc3339(&stamp)
        .map_err(|_| invalid("血缘证据时间无效"))?
        .with_timezone(&Utc);
    Ok(LineageEdge {
        id: id.into(),
        version,
        source_object_id: source,
        target_object_id: target,
        source_object_version: version_value("source_object_version")?,
        target_object_version: version_value("target_object_version")?,
        origin: "manual".into(),
        evidence: text("evidence", 8192)?,
        evidence_actor_id: actor,
        evidence_recorded_at,
        stale: false,
        invalid_reason: None,
    })
}
fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
