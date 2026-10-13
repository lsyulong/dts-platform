use crate::models::error::AppError;
use dts_backend::{ProductResource, repo::development_revisions::DevelopmentRevision};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct DefinitionDifference {
    pub path: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}
#[derive(Debug, Serialize)]
pub struct DevelopmentRevisionDiff {
    pub tenant_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub from_revision_id: String,
    pub to_revision_id: Option<String>,
    pub from_version: i64,
    pub to_version: i64,
    pub from_hash: String,
    pub to_hash: String,
    pub changes: Vec<DefinitionDifference>,
    pub truncated: bool,
    pub comparison_scope: &'static str,
}
pub(super) fn compare(
    from: DevelopmentRevision,
    to: ProductResource,
    to_revision_id: Option<String>,
) -> Result<DevelopmentRevisionDiff, AppError> {
    let mut changes = Vec::new();
    let mut truncated = false;
    let before = editable(&from.snapshot)?;
    let after = editable(&to)?;
    compare_values(
        "",
        Some(&before),
        Some(&after),
        &mut changes,
        &mut truncated,
        0,
    );
    Ok(DevelopmentRevisionDiff {
        tenant_id: to.tenant_id.clone(),
        source_kind: to.kind.clone(),
        source_id: to.id.clone(),
        from_revision_id: from.summary.id,
        to_revision_id,
        from_version: from.snapshot.version,
        to_version: to.version,
        from_hash: from.summary.snapshot_hash,
        to_hash: dts_backend::repo::development_revisions::validation::snapshot_hash(&to)?,
        changes,
        truncated,
        comparison_scope: "editable_definition",
    })
}
fn editable(r: &ProductResource) -> Result<Value, AppError> {
    let value = serde_json::json!({"code":r.code,"name":r.name,"description":r.description,"parent_id":r.parent_id,"properties":r.properties,"references":r.references});
    if value.to_string().len() > 320 * 1024 {
        return Err(AppError::BadRequest("对比定义超过范围".into()));
    }
    Ok(value)
}
fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
fn compare_values(
    path: &str,
    a: Option<&Value>,
    b: Option<&Value>,
    out: &mut Vec<DefinitionDifference>,
    truncated: &mut bool,
    depth: usize,
) {
    if a == b {
        return;
    }
    if out.len() >= 256 {
        *truncated = true;
        return;
    }
    if depth < 24 {
        if let (Some(Value::Object(a)), Some(Value::Object(b))) = (a, b) {
            let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
            for key in keys {
                compare_values(
                    &format!("{path}/{}", escape(key)),
                    a.get(key),
                    b.get(key),
                    out,
                    truncated,
                    depth + 1,
                );
            }
            return;
        }
        if let (Some(Value::Array(a)), Some(Value::Array(b))) = (a, b) {
            for i in 0..a.len().max(b.len()) {
                compare_values(
                    &format!("{path}/{i}"),
                    a.get(i),
                    b.get(i),
                    out,
                    truncated,
                    depth + 1,
                );
            }
            return;
        }
    }
    out.push(DefinitionDifference {
        path: path.into(),
        before: a.cloned(),
        after: b.cloned(),
    });
}
