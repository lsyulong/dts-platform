use std::collections::BTreeSet;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::*;

pub fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}

pub fn text<'a>(properties: &'a Value, key: &str) -> Result<&'a str, BackendError> {
    properties[key]
        .as_str()
        .filter(|v| !v.trim().is_empty() && v.len() <= 128 && !v.chars().any(char::is_control))
        .ok_or_else(|| invalid("资产引用必须是实际对象ID"))
}

pub fn hash(properties: &Value) -> Result<String, BackendError> {
    let bytes = serde_json::to_vec(properties).map_err(|_| invalid("资产来源无法编码"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn references(kind: &str, properties: &Value) -> Result<Vec<ResourceReference>, BackendError> {
    let allowed: &[&str] = match kind {
        "asset_entry" => &["object_kind", "object_id", "domain_id", "owner_id", "tags"],
        "asset_map" => &["domain_id", "asset_ids", "layout"],
        _ => return Err(invalid("不是可编辑的资产定义")),
    };
    if properties
        .as_object()
        .is_none_or(|p| p.keys().any(|k| !allowed.contains(&k.as_str())))
        || serde_json::to_vec(properties)
            .map_err(|_| invalid("资产属性无法编码"))?
            .len()
            > 256 * 1024
    {
        return Err(invalid("资产属性包含未支持字段或超过256KiB"));
    }
    let mut refs = Vec::new();
    if let Some(domain) = properties
        .get("domain_id")
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
    {
        if domain.as_str().is_none() {
            return Err(invalid("数据域必须是实际ID"));
        }
        refs.push(reference(
            "domain_id",
            "warehouse_domain",
            text(properties, "domain_id")?,
        ));
    }
    if kind == "asset_entry" {
        let source_kind = text(properties, "object_kind")?;
        if !ASSET_SOURCE_KINDS.contains(&source_kind) {
            return Err(invalid("不支持的资产来源类型"));
        }
        refs.push(reference(
            "object_id",
            source_kind,
            text(properties, "object_id")?,
        ));
        refs.push(reference("owner_id", "user", text(properties, "owner_id")?));
        if let Some(tags) = properties.get("tags").filter(|v| !v.is_null()) {
            let tags = tags
                .as_array()
                .ok_or_else(|| invalid("标签必须是字符串列表"))?;
            if tags.len() > 100
                || tags.iter().any(|v| {
                    v.as_str().is_none_or(|v| {
                        v.trim().is_empty() || v.len() > 128 || v.chars().any(char::is_control)
                    })
                })
            {
                return Err(invalid("标签列表无效或过长"));
            }
        }
    } else {
        let ids = asset_ids(properties)?;
        refs.extend(
            ids.iter()
                .map(|id| reference("asset_ids", "asset_entry", id)),
        );
        if properties
            .get("layout")
            .is_some_and(|v| !v.is_null() && !v.is_object())
        {
            return Err(invalid("地图布局必须是对象"));
        }
    }
    Ok(refs)
}

pub fn asset_ids(properties: &Value) -> Result<Vec<String>, BackendError> {
    let ids = properties["asset_ids"]
        .as_array()
        .ok_or_else(|| invalid("资产节点必须是实际ID列表"))?;
    if ids.is_empty() || ids.len() > 500 {
        return Err(invalid("资产地图需要1到500个实际节点"));
    }
    let mut unique = BTreeSet::new();
    for id in ids {
        let id = id
            .as_str()
            .filter(|v| !v.trim().is_empty() && v.len() <= 128 && !v.chars().any(char::is_control))
            .ok_or_else(|| invalid("资产节点ID无效"))?;
        if !unique.insert(id.to_owned()) {
            return Err(invalid("资产节点不能重复"));
        }
    }
    Ok(unique.into_iter().collect())
}

pub fn fields(params: &SaveAssetParams) -> Result<(), BackendError> {
    let f = &params.fields;
    if f.parent_id.is_some()
        || f.status != "draft"
        || f.code.is_empty()
        || f.code.len() > 128
        || !f
            .code
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        || (!f.code.as_bytes()[0].is_ascii_alphabetic() && !f.code.starts_with('_'))
        || f.name.trim().is_empty()
        || f.name.chars().count() > 128
        || f.name.chars().any(char::is_control)
        || f.description.as_ref().is_some_and(|v| v.len() > 8192)
    {
        return Err(invalid("资产编码、名称、状态或父级无效"));
    }
    let expected = references(&params.kind, &f.properties)?;
    if f.references.len() != expected.len() || expected.iter().any(|r| !f.references.contains(r)) {
        return Err(invalid("资产属性与真实引用不一致"));
    }
    Ok(())
}

pub fn usage(request: &AssetUsageRequest) -> Result<(), BackendError> {
    if request.asset_id.is_empty()
        || request.asset_id.len() > 128
        || request.asset_id.chars().any(char::is_control)
        || request.period_start >= request.period_end
        || request.period_end - request.period_start > chrono::Duration::days(366)
        || request.period_end > Utc::now() + chrono::Duration::minutes(1)
    {
        return Err(invalid("使用统计要求实际资产及不超过366天的过去时间窗口"));
    }
    Ok(())
}
