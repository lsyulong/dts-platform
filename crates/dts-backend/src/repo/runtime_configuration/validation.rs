use super::*;
use crate::{BackendError, ProductResource};
use serde_json::{Value, json};

pub fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
pub fn text(value: &str, maximum: usize) -> Result<(), BackendError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(invalid("运行配置标识或原因为空、过长或包含控制字符"));
    }
    Ok(())
}
pub fn scope(s: &RuntimeConfigurationScope) -> Result<(), BackendError> {
    text(&s.tenant_id, 128)?;
    text(&s.actor_id, 128)
}
pub fn kind(kind: &str) -> Result<(), BackendError> {
    if !matches!(kind, "sql_document" | "development_workflow") {
        return Err(invalid("运行配置仅支持 SQL 文档和开发工作流"));
    }
    Ok(())
}
pub fn source(s: &RuntimeSourceLookup) -> Result<(), BackendError> {
    scope(&s.scope)?;
    kind(&s.source_kind)?;
    text(&s.source_id, 128)?;
    if s.source_version < 1 {
        return Err(invalid("来源版本无效"));
    }
    Ok(())
}
pub fn request(version: i64, reason: &str, key: &str) -> Result<(), BackendError> {
    if version < 1 {
        return Err(invalid("配置版本无效"));
    }
    text(reason, 8192)?;
    uuid::Uuid::parse_str(key).map_err(|_| invalid("必须提供稳定 UUID 幂等键"))?;
    Ok(())
}
pub fn resource_hash(resource: &ProductResource) -> Result<String, BackendError> {
    crate::repo::development_revisions::validation::snapshot_hash(resource)
}
pub fn sensitive_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase().replace('_', "");
    name == "pwd"
        || [
            "password",
            "passwd",
            "secret",
            "token",
            "authorization",
            "credential",
            "apikey",
            "privatekey",
            "connectionstring",
            "dsn",
            "salt",
        ]
        .iter()
        .any(|part| name.contains(part))
}
fn secret_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.trim_start().starts_with("bearer ")
        || lower.starts_with("credential:")
        || (lower.contains("-----begin") && lower.contains("key-----"))
        || ["password=", "passwd=", "token=", "secret=", "api_key="]
            .iter()
            .any(|key| lower.contains(key))
        || value.split_once("://").is_some_and(|(_, rest)| {
            rest.split(['/', '?', '#'])
                .next()
                .is_some_and(|authority| authority.contains('@'))
        })
        || (value.starts_with("eyJ") && value.split('.').count() == 3)
}
pub fn variables(value: &Value) -> Result<(), BackendError> {
    let map = value
        .as_object()
        .ok_or_else(|| invalid("运行变量必须是对象"))?;
    if map.len() > 32 || value.to_string().len() > 16 * 1024 {
        return Err(invalid("变量超过 32 项或 16 KiB"));
    }
    for (name, value) in map {
        if name.is_empty()
            || name.len() > 80
            || !name.as_bytes()[0].is_ascii_alphabetic()
            || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || sensitive_name(name)
        {
            return Err(invalid(
                "变量名无效或包含凭据名称；当前只读执行器不消费运行凭据",
            ));
        }
        if let Some(text) = value.as_str() {
            if text.len() > 4096 || text.chars().any(char::is_control) || secret_value(text) {
                return Err(invalid("变量不能包含秘密、凭据引用或无效文本"));
            }
        } else if !value.is_null()
            && !value.is_boolean()
            && !value.as_f64().is_some_and(f64::is_finite)
        {
            return Err(invalid("运行变量仅允许标量参数值"));
        }
    }
    Ok(())
}
pub fn policy(value: &Value) -> Result<RuntimeConfigurationPolicy, BackendError> {
    if value.to_string().len() > 256 * 1024 {
        return Err(invalid("运行配置超过 256 KiB"));
    }
    let p: RuntimeConfigurationPolicy = serde_json::from_value(value.clone())
        .map_err(|_| invalid("运行配置结构无效，scope 必须为 source 或 shared"))?;
    variables(&p.variables)?;
    if p.query_timeout_seconds
        .is_some_and(|seconds| !(1..=60).contains(&seconds))
        || p.concurrency.is_some_and(|cap| !(1..=16).contains(&cap))
    {
        return Err(invalid(
            "单次查询超时必须为 1..60 秒，并发租约上限必须为 1..16",
        ));
    }
    match p.scope {
        RuntimeConfigurationRange::Shared if p.source_kind.is_some() || p.source_id.is_some() => {
            return Err(invalid("共享配置不能附带来源身份"));
        },
        RuntimeConfigurationRange::Source => {
            kind(
                p.source_kind
                    .as_deref()
                    .ok_or_else(|| invalid("source 配置缺少实际来源类型"))?,
            )?;
            text(
                p.source_id
                    .as_deref()
                    .ok_or_else(|| invalid("source 配置缺少实际来源 ID"))?,
                128,
            )?;
        },
        _ => {},
    }
    Ok(p)
}
pub fn evidence(
    configuration: &ProductResource,
) -> Result<RuntimeConfigurationEvidence, BackendError> {
    if configuration.kind != "development_config" || configuration.status != "published" {
        return Err(BackendError::Conflict("配置未发布或已撤回".into()));
    }
    let p = policy(&configuration.properties)?;
    Ok(RuntimeConfigurationEvidence {
        tenant_id: configuration.tenant_id.clone(),
        configuration_id: configuration.id.clone(),
        configuration_version: configuration.version,
        configuration_hash: resource_hash(configuration)?,
        query_timeout_seconds: p.query_timeout_seconds.unwrap_or(60),
        concurrency: p.concurrency.unwrap_or(1),
        scope: p.scope,
        source_kind: p.source_kind,
        source_id: p.source_id,
    })
}
pub fn configuration_id(source: &ProductResource) -> Result<Option<String>, BackendError> {
    match source.properties.get("configuration_id") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(id)) => {
            text(id, 128)?;
            Ok(Some(id.clone()))
        },
        _ => Err(invalid("configuration_id 必须引用真实配置 ID")),
    }
}
pub fn matches_source(
    e: &RuntimeConfigurationEvidence,
    source: &ProductResource,
) -> Result<(), BackendError> {
    if e.tenant_id != source.tenant_id
        || (e.scope == RuntimeConfigurationRange::Source
            && (e.source_kind.as_deref() != Some(source.kind.as_str())
                || e.source_id.as_deref() != Some(source.id.as_str())))
    {
        return Err(BackendError::Conflict(
            "配置范围与实际开发来源不匹配".into(),
        ));
    }
    Ok(())
}
pub fn semantic(id: &str, version: i64, reason: &str) -> Value {
    json!({"configuration_id":id,"expected_version":version,"reason":reason})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_rejects_unpaired_scope_limits_and_credentials() {
        assert!(policy(&json!({"scope":"shared","variables":{},"concurrency":1})).is_ok());
        for value in [
            json!({"scope":"tenant","variables":{}}),
            json!({"scope":"source","source_kind":"sql_document","variables":{}}),
            json!({"scope":"shared","source_id":"x","variables":{}}),
            json!({"scope":"shared","variables":{},"timeout_seconds":61}),
            json!({"scope":"shared","variables":{},"concurrency":1.5}),
            json!({"scope":"shared","variables":{"db_credential_ref":"credential:47c941eb-3856-4e01-9ee7-cedf7d600131"}}),
            json!({"scope":"shared","variables":{"value":"bearer secret"}}),
        ] {
            assert!(policy(&value).is_err(), "{value}");
        }
    }
    #[test]
    fn variables_are_bounded_non_secret_scalars() {
        assert!(
            variables(&json!({"count":"9007199254740993","date":"2026-10-09","enabled":true}))
                .is_ok()
        );
        assert!(variables(&json!({"nested":{"db_password":"secret"}})).is_err());
        assert!(variables(&json!({"value":"postgres://user:password@host/db"})).is_err());
        assert!(
            variables(&json!({"value":"credential:47c941eb-3856-4e01-9ee7-cedf7d600131"})).is_err()
        );
    }
}
