use super::storage::*;
use dts_backend::{BackendError, repo::standard_governance::*};
use sqlx::PgConnection;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn direct(binding: &ModelStandardBindingSet) -> Vec<FrozenStandardRef> {
    binding
        .columns
        .iter()
        .flat_map(|column| {
            [
                column.field_standard.clone(),
                column.naming_standard.clone(),
            ]
        })
        .chain(std::iter::once(binding.table_naming.clone()))
        .flatten()
        .collect()
}
pub(super) fn keys(
    refs: &[FrozenStandardRef],
) -> Result<BTreeMap<(String, String, i64), String>, BackendError> {
    if refs.len() > 2001 {
        return Err(invalid("标准原始引用超过2001项"));
    }
    let mut values = BTreeMap::new();
    for reference in refs {
        kind(&reference.kind)?;
        text(&reference.id)?;
        version(reference.revision_version)?;
        if reference.definition_hash.len() != 64
            || !reference
                .definition_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid("标准修订hash格式无效"));
        }
        let key = (
            reference.kind.clone(),
            reference.id.clone(),
            reference.revision_version,
        );
        if let Some(old) = values.insert(key, reference.definition_hash.clone()) {
            if old != reference.definition_hash {
                return Err(conflict("同一标准修订绑定了不同hash"));
            }
        }
    }
    if values.len() > 256 {
        return Err(invalid("独立标准依赖超过256项"));
    }
    Ok(values)
}
pub(super) async fn active(
    conn: &mut PgConnection,
    tenant: &str,
    refs: &[FrozenStandardRef],
) -> Result<Vec<StandardRevision>, BackendError> {
    let mut stack = keys(refs)?
        .into_iter()
        .map(|((kind, id, version), hash)| (kind, id, version, hash, 0usize))
        .collect::<Vec<_>>();
    let mut visited = BTreeMap::new();
    let mut bytes = 0usize;
    while let Some((kind, id, version, hash, depth)) = stack.pop() {
        if depth > 8 || visited.len() > 256 {
            return Err(invalid("标准依赖层级或数量超过容量"));
        }
        let key = (kind.clone(), id.clone(), version);
        if let Some(revision) = visited.get(&key) {
            let revision: &StandardRevision = revision;
            if revision.definition_hash != hash {
                return Err(conflict("标准修订hash不一致"));
            }
            continue;
        }
        if visited.len() >= 256 {
            return Err(invalid("递归独立标准依赖超过256项"));
        }
        let revision = revision(conn, tenant, &kind, &id, version)
            .await?
            .ok_or_else(|| conflict("绑定的不可变标准修订不存在"))?;
        if revision.state != "active" || revision.definition_hash != hash {
            return Err(conflict("标准修订已撤回或hash已失效"));
        }
        validate_children(&revision.properties, &revision.kind, &revision.children)?;
        bytes = bytes
            .checked_add(encode(&revision)?.len())
            .ok_or_else(|| invalid("标准依赖证据超限"))?;
        if bytes > 1024 * 1024 {
            return Err(invalid("标准依赖证据超过1MiB"));
        }
        for child in &revision.children {
            stack.push((
                child.kind.clone(),
                child.id.clone(),
                child.revision_version,
                child.definition_hash.clone(),
                depth + 1,
            ));
        }
        visited.insert(key, revision);
    }
    Ok(visited.into_values().collect())
}
pub(super) fn validate_children(
    properties: &serde_json::Value,
    kind: &str,
    children: &[FrozenStandardRef],
) -> Result<(), BackendError> {
    if kind != "field_standard" {
        if children.is_empty() {
            return Ok(());
        }
        return Err(invalid("只有字段标准可以引用单位或枚举修订"));
    }
    let mut expected = BTreeSet::new();
    for (key, version_key, child_kind) in [
        ("unit_id", "unit_version", "unit_standard"),
        ("enum_id", "enum_version", "enum_standard"),
    ] {
        match (
            properties.get(key).filter(|value| !value.is_null()),
            properties.get(version_key).filter(|value| !value.is_null()),
        ) {
            (None, None) => {},
            (Some(id), Some(version)) => {
                let id = id
                    .as_str()
                    .ok_or_else(|| invalid("字段标准子引用必须是真实标识"))?;
                let version = version
                    .as_i64()
                    .ok_or_else(|| invalid("字段标准子修订版本无效"))?;
                expected.insert((child_kind.to_string(), id.to_string(), version));
            },
            _ => return Err(invalid("字段标准子引用标识与修订版本必须成对")),
        }
    }
    let actual = keys(children)?.into_keys().collect::<BTreeSet<_>>();
    if actual != expected || actual.len() != children.len() {
        return Err(invalid("字段标准单位/枚举修订与冻结子引用不一致"));
    }
    Ok(())
}
pub(super) async fn for_binding(
    conn: &mut PgConnection,
    tenant: &str,
    binding: &ModelStandardBindingSet,
) -> Result<Vec<StandardRevision>, BackendError> {
    active(conn, tenant, &direct(binding)).await
}
