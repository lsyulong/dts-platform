use super::*;
use crate::BackendError;
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
pub fn target_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub fn bounded(value: &str, max: usize) -> Result<(), BackendError> {
    if value.is_empty()
        || value.len() > max
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(invalid("服务身份标识或文本无效"))
    } else {
        Ok(())
    }
}
pub fn version(value: i64) -> Result<(), BackendError> {
    if value < 1 || value == i64::MAX {
        Err(invalid("服务身份版本无效"))
    } else {
        Ok(())
    }
}
pub fn expiry(value: DateTime<Utc>) -> Result<(), BackendError> {
    let now = Utc::now();
    if value <= now || value > now + Duration::days(365) {
        Err(invalid("有效期须在未来且不超过365天"))
    } else {
        Ok(())
    }
}
pub fn pagination(limit: u32, offset: u32) -> Result<(), BackendError> {
    if limit == 0 || limit > 100 || offset > 1_000_000 {
        Err(invalid("服务身份分页范围无效"))
    } else {
        Ok(())
    }
}
pub fn search_pattern(q: Option<&str>) -> Result<Option<String>, BackendError> {
    let Some(q) = q else {
        return Ok(None);
    };
    if q.chars().count() > 128 || q.chars().any(char::is_control) {
        return Err(invalid("搜索文本不能包含控制字符或超过128字"));
    }
    let q = q.trim();
    if q.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!(
        "%{}%",
        q.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )))
}
pub fn verifier(value: &str) -> Result<(), BackendError> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Err(invalid("服务凭据校验摘要无效"))
    } else {
        Ok(())
    }
}

/// Only a concrete HTTP(S) origin; no userinfo, path, query, fragment, wildcard,
/// encoded host or IPv6 zone. Canonical default ports are removed.
pub fn canonical_origin(value: &str) -> Result<String, BackendError> {
    bounded(value, 512)?;
    let (scheme, authority) = value
        .split_once("://")
        .ok_or_else(|| invalid("来源必须是HTTP(S)origin"))?;
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https")
        || authority.is_empty()
        || authority.chars().any(|c| "/@?#%\\".contains(c))
    {
        return Err(invalid(
            "来源只接受完整HTTP(S)origin，不包含路径、用户信息或通配符",
        ));
    }
    let (host, port) = if authority.starts_with('[') {
        let end = authority.find(']').ok_or_else(|| invalid("IPv6来源无效"))?;
        let ip: std::net::Ipv6Addr = authority[1..end]
            .parse()
            .map_err(|_| invalid("IPv6来源无效"))?;
        let rest = &authority[end + 1..];
        let port = if rest.is_empty() {
            None
        } else {
            Some(
                rest.strip_prefix(':')
                    .ok_or_else(|| invalid("来源端口无效"))?,
            )
        };
        (format!("[{ip}]"), port)
    } else {
        let (host, port) = authority
            .split_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)));
        let host = host.to_ascii_lowercase();
        if host.len() > 253
            || host.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err(invalid("来源主机名无效"));
        }
        if host.bytes().all(|b| b.is_ascii_digit() || b == b'.')
            && host.parse::<std::net::Ipv4Addr>().is_err()
        {
            return Err(invalid("IPv4来源无效"));
        }
        (host, port)
    };
    let port = port
        .map(|port| {
            port.parse::<u16>()
                .ok()
                .filter(|p| *p > 0)
                .ok_or_else(|| invalid("来源端口须为1至65535"))
        })
        .transpose()?;
    let port = port
        .filter(|port| !((scheme == "http" && *port == 80) || (scheme == "https" && *port == 443)))
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    Ok(format!("{scheme}://{host}{port}"))
}
pub fn origins(values: &[String]) -> Result<(), BackendError> {
    if values.len() > 100 {
        return Err(invalid("来源白名单最多100项"));
    }
    let mut unique = BTreeSet::new();
    for value in values {
        let canonical = canonical_origin(value)?;
        if canonical != *value || !unique.insert(canonical) {
            return Err(invalid("来源必须采用规范origin且不能重复"));
        }
    }
    Ok(())
}
pub fn save_application(params: &SaveServiceApplicationParams) -> Result<(), BackendError> {
    for id in [
        &params.id,
        &params.tenant_id,
        &params.actor_id,
        &params.owner_id,
    ] {
        bounded(id, 128)?;
    }
    basic_fields(
        &params.code,
        &params.name,
        &params.description,
        params.expected_version,
    )?;
    origins(&params.allowed_origins)?;
    expiry(params.credential_expires_at)
}
pub fn application_properties(params: &SaveServiceApplicationParams) -> Value {
    json!({"owner_id":params.owner_id,"allowed_origins":params.allowed_origins,"credential_expires_at":params.credential_expires_at})
}
fn basic_fields(
    code: &str,
    name: &str,
    description: &Option<String>,
    expected: Option<i64>,
) -> Result<(), BackendError> {
    bounded(code, 128)?;
    bounded(name, 512)?;
    if description
        .as_ref()
        .is_some_and(|v| v.len() > 8192 || v.contains('\0'))
    {
        return Err(invalid("说明无效或过长"));
    }
    if let Some(value) = expected {
        version(value)?;
    }
    Ok(())
}
pub fn save_subscription(params: &SaveServiceSubscriptionParams) -> Result<(), BackendError> {
    for id in [
        &params.id,
        &params.tenant_id,
        &params.actor_id,
        &params.application_id,
        &params.api_id,
    ] {
        bounded(id, 128)?;
    }
    basic_fields(
        &params.code,
        &params.name,
        &params.description,
        params.expected_version,
    )?;
    version(params.api_version)?;
    if params.purpose.trim().is_empty()
        || params.purpose.len() > 8192
        || params.purpose.contains('\0')
    {
        return Err(invalid("订阅目的无效或过长"));
    }
    expiry(params.expires_at)
}
pub fn subscription_properties(
    params: &SaveServiceSubscriptionParams,
    owner: &str,
    requester: &str,
) -> Value {
    json!({"application_id":params.application_id,"api_id":params.api_id,"api_version":params.api_version,
        "owner_id":owner,"requester_id":requester,"purpose":params.purpose,"expires_at":params.expires_at})
}
pub fn issue_semantics(params: &IssueServiceCredentialParams) -> Result<Value, BackendError> {
    for id in [
        &params.tenant_id,
        &params.actor_id,
        &params.application_id,
        &params.idempotency_key,
    ] {
        bounded(id, 128)?;
    }
    version(params.expected_version)?;
    Ok(
        json!({"action":"issue_credential","actor_id":params.actor_id,"application_id":params.application_id,
        "expected_version":params.expected_version,"requested_expires_at":params.requested_expires_at}),
    )
}
pub fn revoke_semantics(params: &RevokeServiceCredentialParams) -> Result<Value, BackendError> {
    for id in [
        &params.tenant_id,
        &params.actor_id,
        &params.application_id,
        &params.credential_id,
        &params.idempotency_key,
    ] {
        bounded(id, 128)?;
    }
    version(params.expected_version)?;
    Ok(
        json!({"action":"revoke_credential","actor_id":params.actor_id,"application_id":params.application_id,
        "credential_id":params.credential_id,"expected_version":params.expected_version}),
    )
}
pub fn decision_semantics(params: &DecideServiceSubscriptionParams) -> Result<Value, BackendError> {
    for id in [
        &params.tenant_id,
        &params.actor_id,
        &params.subscription_id,
        &params.idempotency_key,
    ] {
        bounded(id, 128)?;
    }
    version(params.expected_version)?;
    if params.reason.len() > 8192
        || params.reason.contains('\0')
        || (params.decision.needs_approver() && params.reason.trim().is_empty())
    {
        return Err(invalid("审批意见无效或未填写"));
    }
    if params.decision != ServiceSubscriptionDecision::Approve && params.expires_at.is_some() {
        return Err(invalid("只有批准可缩短有效期"));
    }
    Ok(
        json!({"actor_id":params.actor_id,"subscription_id":params.subscription_id,"expected_version":params.expected_version,
        "action":params.decision.as_str(),"reason":params.reason,"expires_at":params.expires_at}),
    )
}
pub fn reserve(params: &ReserveServiceCallParams) -> Result<(), BackendError> {
    for id in [
        &params.call_id,
        &params.tenant_id,
        &params.credential_id,
        &params.api_id,
    ] {
        bounded(id, 128)?;
    }
    version(params.credential_version)?;
    verifier(&params.credential_verifier)?;
    if let Some(origin) = &params.origin {
        // Malformed origins are a real authenticated policy denial in reserve,
        // never matched to a canonical application allowlist or stored as success.
        bounded(origin, 512)?;
    }
    Ok(())
}
pub fn finish(params: &FinishServiceCallParams) -> Result<(), BackendError> {
    for id in [
        &params.call.id,
        &params.call.tenant_id,
        &params.call.owner_id,
        &params.call.application_id,
        &params.call.credential_id,
        &params.call.subscription_id,
        &params.call.api_id,
    ] {
        bounded(id, 128)?;
    }
    if params.outcome == ServiceCallOutcome::Pending
        || params.duration_ms > 86_400_000
        || params.row_count.is_some_and(|rows| rows > 10_000)
        || (params.outcome == ServiceCallOutcome::Succeeded) != params.row_count.is_some()
    {
        return Err(invalid("调用结果、实际行数或耗时无效"));
    }
    if params.access_audit.is_some() != params.masking_versions.is_some() {
        return Err(invalid("成功访问审计与隐私策略版本必须同时提供"));
    }
    if let Some(versions) = &params.masking_versions {
        if versions.len() > 2000 {
            return Err(invalid("隐私策略版本超过容量"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for policy in versions {
            bounded(&policy.id, 128)?;
            version(policy.version)?;
            if !matches!(policy.kind.as_str(), "masking_rule" | "user_group")
                || !seen.insert((&policy.kind, &policy.id))
            {
                return Err(invalid("隐私策略种类或版本身份重复"));
            }
        }
    }
    if let Some(audit) = &params.access_audit {
        crate::repo::data_access::validation::evidence(audit)?;
        if params.outcome != ServiceCallOutcome::Succeeded
            || audit.outcome.as_str() != "succeeded"
            || audit.tenant_id != params.call.tenant_id
            || audit.actor_id != params.call.owner_id
            || audit.id != format!("service_call:{}", params.call.id)
        {
            return Err(invalid("访问审计不属于实际成功调用和授权身份"));
        }
    }
    Ok(())
}
