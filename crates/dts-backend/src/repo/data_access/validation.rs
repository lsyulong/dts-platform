use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use super::*;

fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}

pub fn query_hash(sql: &str) -> String {
    format!("{:x}", Sha256::digest(sql.as_bytes()))
}

pub fn bounded(value: &str, max: usize) -> Result<(), BackendError> {
    if value.trim().is_empty()
        || value.len() > max
        || value != value.trim()
        || value.chars().any(char::is_control)
    {
        Err(invalid("数据授权标识或文本无效"))
    } else {
        Ok(())
    }
}

pub fn target(
    target: &DataAccessTarget,
    tenant: &str,
    requested: &[String],
) -> Result<(), BackendError> {
    bounded(&target.metadata_id, 128)?;
    let binding = &target.binding;
    if binding.engine != "swanlake"
        || binding.tenant_id != tenant
        || binding.object_version < 1
        || binding.source_fingerprint.len() != 64
        || !binding
            .source_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || target.columns.is_empty()
        || target.columns.len() > 1000
    {
        return Err(invalid("数据授权目标缺少真实引擎绑定、版本或字段"));
    }
    for value in [
        &binding.table_catalog,
        &binding.schema_name,
        &binding.table_name,
    ] {
        bounded(value, 256)?;
    }
    let mut names = HashSet::new();
    let mut previous = 0;
    for column in &target.columns {
        bounded(&column.name, 256)?;
        bounded(&column.data_type, 512)?;
        if !names.insert(column.name.to_ascii_lowercase()) || column.ordinal_position <= previous {
            return Err(invalid("冻结字段重复或顺序无效"));
        }
        previous = column.ordinal_position;
    }
    if requested.is_empty() || requested.len() > target.columns.len() {
        return Err(invalid("请选择至少一个真实字段"));
    }
    let mut selected = HashSet::new();
    for name in requested {
        if !selected.insert(name.to_ascii_lowercase())
            || !target.columns.iter().any(|column| &column.name == name)
        {
            return Err(invalid("请求字段重复或不在冻结元数据中"));
        }
    }
    Ok(())
}

pub fn save(params: &SaveDataAccessRequestParams) -> Result<(), BackendError> {
    for id in [&params.id, &params.tenant_id, &params.actor_id] {
        bounded(id, 128)?;
    }
    bounded(&params.code, 128)?;
    bounded(&params.name, 512)?;
    if params
        .description
        .as_ref()
        .is_some_and(|description| description.len() > 8192 || description.contains('\0'))
        || params.purpose.trim().is_empty()
        || params.purpose.len() > 8192
        || params.purpose.contains('\0')
    {
        return Err(invalid("申请说明无效或过长"));
    }
    if params
        .expected_version
        .is_some_and(|version| version < 1 || version == i64::MAX)
    {
        return Err(invalid("申请版本无效"));
    }
    expiry(params.expires_at)?;
    target(&params.target, &params.tenant_id, &params.requested_columns)
}

pub fn expiry(expires_at: chrono::DateTime<Utc>) -> Result<(), BackendError> {
    let now = Utc::now();
    if expires_at <= now || expires_at > now + Duration::days(365) {
        Err(invalid("授权失效时间须在未来且不超过365天"))
    } else {
        Ok(())
    }
}

/// The receipt compares client semantics, excluding generated IDs and current time.
pub fn decision_request(params: &DecideDataAccessParams) -> Result<Value, BackendError> {
    for id in [
        &params.tenant_id,
        &params.request_id,
        &params.actor_id,
        &params.idempotency_key,
    ] {
        bounded(id, 128)?;
    }
    if params.expected_version < 1
        || params.expected_version == i64::MAX
        || params.reason.len() > 8192
        || params.reason.contains('\0')
        || (params.decision.needs_approver() && params.reason.trim().is_empty())
    {
        return Err(invalid("审批版本或必填意见无效"));
    }
    if params.decision != DataAccessDecision::Approve && params.expires_at.is_some() {
        return Err(invalid("仅审批授权可以指定失效时间"));
    }
    Ok(
        json!({"request_id":params.request_id,"expected_version":params.expected_version,"actor_id":params.actor_id,
        "action":params.decision.as_str(),"reason":params.reason,"expires_at":params.expires_at}),
    )
}

pub fn request_properties(params: &SaveDataAccessRequestParams) -> Value {
    json!({"object_id":params.target.metadata_id,"columns":params.requested_columns,"purpose":params.purpose,"expires_at":params.expires_at,
        "requester_id":params.actor_id,"metadata_version":params.target.binding.object_version})
}

pub fn evidence(params: &RecordDataAccessParams) -> Result<(), BackendError> {
    for id in [&params.id, &params.tenant_id, &params.actor_id] {
        bounded(id, 128)?;
    }
    let evidence = &params.evidence;
    if evidence.query_hash.len() != 64
        || !evidence
            .query_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || evidence.grant_ids.len() > 1000
        || evidence.metadata_ids.len() > 1000
        || evidence.columns.len() > 1000
    {
        return Err(invalid("访问审计证据格式无效"));
    }
    for id in evidence.grant_ids.iter().chain(&evidence.metadata_ids) {
        bounded(id, 128)?;
    }
    for column in &evidence.columns {
        bounded(column, 256)?;
    }
    Ok(())
}
