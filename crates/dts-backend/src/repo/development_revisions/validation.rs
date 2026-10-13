use super::DevelopmentRevisionScope;
use crate::{BackendError, ProductResource};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
pub fn scope(s: &DevelopmentRevisionScope) -> Result<(), BackendError> {
    for id in [&s.tenant_id, &s.actor_id, &s.resource_id] {
        identifier(id)?;
    }
    if !matches!(s.kind.as_str(), "sql_document" | "development_workflow") {
        return Err(invalid("修订仅支持 SQL 文档和开发工作流"));
    }
    Ok(())
}
pub fn identifier(id: &str) -> Result<(), BackendError> {
    if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
        return Err(invalid("修订标识无效"));
    }
    Ok(())
}
pub fn write(
    s: &DevelopmentRevisionScope,
    version: i64,
    reason: &str,
    key: &str,
) -> Result<(), BackendError> {
    scope(s)?;
    if version < 1
        || reason.trim().is_empty()
        || reason.len() > 8192
        || reason.chars().any(char::is_control)
        || uuid::Uuid::parse_str(key).is_err()
    {
        return Err(invalid("修订须有实际版本、理由和稳定 UUID"));
    }
    Ok(())
}
pub fn page(limit: u32, offset: u32) -> Result<(), BackendError> {
    if !(1..=100).contains(&limit) || offset > 1_000_000 {
        return Err(invalid("修订分页超出范围"));
    }
    Ok(())
}
pub fn canonical_snapshot(snapshot: &ProductResource) -> Result<ProductResource, BackendError> {
    let mut value = snapshot.clone();
    value.references.sort_by(|a, b| {
        (&a.field, &a.target_kind, &a.target_id).cmp(&(&b.field, &b.target_kind, &b.target_id))
    });
    if serde_json::to_vec(&value)
        .map_err(|_| invalid("修订快照无法编码"))?
        .len()
        > 320 * 1024
    {
        return Err(invalid("修订完整快照超过 320 KiB"));
    }
    Ok(value)
}
pub fn snapshot_hash(snapshot: &ProductResource) -> Result<String, BackendError> {
    let snapshot = canonical_snapshot(snapshot)?;
    let value = serde_json::to_value(&snapshot).map_err(|_| invalid("修订快照无法编码"))?;
    let bytes = serde_json::to_vec(&ordered(value)).map_err(|_| invalid("修订快照无法编码"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn ordered(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let entries: std::collections::BTreeMap<_, _> = map.into_iter().collect();
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, ordered(value)))
                    .collect(),
            )
        },
        Value::Array(values) => Value::Array(values.into_iter().map(ordered).collect()),
        scalar => scalar,
    }
}
pub fn semantic(
    s: &DevelopmentRevisionScope,
    version: i64,
    reason: &str,
    revision_id: Option<&str>,
) -> Value {
    json!({"actor_id":s.actor_id,"kind":s.kind,"resource_id":s.resource_id,"expected_version":version,"reason":reason,"revision_id":revision_id})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn write_and_scope_reject_unknown_kind_empty_reason_and_non_uuid() {
        let mut s = DevelopmentRevisionScope {
            tenant_id: "t".into(),
            actor_id: "a".into(),
            kind: "sql_document".into(),
            resource_id: "r".into(),
        };
        let key = uuid::Uuid::new_v4().to_string();
        assert!(write(&s, 1, "保存当前定义", &key).is_ok());
        assert!(write(&s, 0, "保存", &key).is_err());
        assert!(write(&s, 1, " \n", &key).is_err());
        assert!(write(&s, 1, "保存", "fake").is_err());
        s.kind = "bi_report".into();
        assert!(scope(&s).is_err());
    }
    #[test]
    fn semantic_includes_actor_version_and_exact_reason() {
        let s = DevelopmentRevisionScope {
            tenant_id: "t".into(),
            actor_id: "a".into(),
            kind: "sql_document".into(),
            resource_id: "r".into(),
        };
        assert_ne!(semantic(&s, 1, "保存", None), semantic(&s, 2, "保存", None));
        assert_ne!(
            semantic(&s, 1, "保存", None),
            semantic(&s, 1, "保存", Some("revision"))
        );
    }
}
