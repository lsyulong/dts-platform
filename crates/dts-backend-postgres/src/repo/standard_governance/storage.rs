use super::super::product_resource::{PgProductResourceRepo as Resources, helpers};
use dts_backend::{BackendError, ProductResource, ResourceWrite, repo::standard_governance::*};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgConnection, Row};

pub(super) fn error(error: sqlx::Error) -> BackendError {
    if let sqlx::Error::Database(db) = &error {
        if matches!(db.code().as_deref(), Some("40001" | "40P01")) {
            return conflict("标准治理事务发生并发冲突，请按原命令重试");
        }
    }
    helpers::map_error(error)
}
pub(super) fn now() -> chrono::DateTime<chrono::Utc> {
    let value = chrono::Utc::now();
    chrono::DateTime::from_timestamp_micros(value.timestamp_micros()).unwrap_or(value)
}
pub(super) fn invalid(text: &str) -> BackendError {
    BackendError::ConstraintViolation(text.into())
}
pub(super) fn conflict(text: &str) -> BackendError {
    BackendError::Conflict(text.into())
}
pub(super) fn encode(value: &impl Serialize) -> Result<String, BackendError> {
    serde_json::to_string(value).map_err(|_| BackendError::Internal("标准证据编码失败".into()))
}
pub(super) fn decode<T: DeserializeOwned>(value: &str) -> Result<T, BackendError> {
    serde_json::from_str(value).map_err(|_| BackendError::Internal("持久化标准治理格式无效".into()))
}
pub(super) fn text(value: &str) -> Result<(), BackendError> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(invalid("标准治理标识无效"))
    } else {
        Ok(())
    }
}
pub(super) fn version(value: i64) -> Result<(), BackendError> {
    if value < 1 || value == i64::MAX {
        Err(invalid("标准治理版本无效"))
    } else {
        Ok(())
    }
}
pub(super) fn kind(value: &str) -> Result<(), BackendError> {
    if STANDARD_KINDS.contains(&value) {
        Ok(())
    } else {
        Err(invalid("标准类型无效"))
    }
}
pub(super) fn page(limit: u32, offset: u32) -> Result<(), BackendError> {
    if !(1..=100).contains(&limit) || offset > 1_000_000 {
        Err(invalid("标准治理分页范围无效"))
    } else {
        Ok(())
    }
}
pub(super) async fn resource(
    conn: &mut PgConnection,
    tenant: &str,
    kind: &str,
    id: &str,
) -> Result<ProductResource, BackendError> {
    sqlx::query(
        "SELECT id FROM product_resource WHERE tenant_id=$1 AND kind=$2 AND id=$3 FOR SHARE",
    )
    .bind(tenant)
    .bind(kind)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(error)?;
    Resources::read_resource(conn, tenant, kind, id)
        .await?
        .ok_or_else(|| BackendError::NotFound("标准或模型不存在或不属于当前租户".into()))
}
pub(super) async fn write(
    conn: &mut PgConnection,
    before: &ProductResource,
    actor: &str,
    properties: Value,
    status: &str,
) -> Result<ProductResource, BackendError> {
    if encode(&properties)?.len() > 262144 {
        return Err(invalid("标准或模型定义超过256KiB"));
    }
    let mut after = before.clone();
    after.version = before
        .version
        .checked_add(1)
        .ok_or_else(|| conflict("资源版本已耗尽"))?;
    after.updated_at = now();
    after.status = status.into();
    after.properties = properties;
    let fields = ResourceWrite {
        code: after.code.clone(),
        name: after.name.clone(),
        description: after.description.clone(),
        parent_id: after.parent_id.clone(),
        status: after.status.clone(),
        properties: after.properties.clone(),
        references: after.references.clone(),
    };
    helpers::validate_graph(conn, &after.tenant_id, &after.kind, &after.id, &fields).await?;
    let changed=sqlx::query("UPDATE product_resource SET properties=$1::jsonb,status=$2,version=$3,updated_at=$4 WHERE tenant_id=$5 AND kind=$6 AND id=$7 AND version=$8").bind(encode(&after.properties)?).bind(&after.status).bind(after.version).bind(after.updated_at).bind(&after.tenant_id).bind(&after.kind).bind(&after.id).bind(before.version).execute(&mut *conn).await.map_err(error)?.rows_affected();
    if changed != 1 {
        return Err(conflict("标准或模型定义版本已改变"));
    }
    helpers::audit(conn, actor, "update", Some(before), Some(&after)).await?;
    Ok(after)
}
pub(super) async fn revision(
    conn: &mut PgConnection,
    tenant: &str,
    kind: &str,
    id: &str,
    version: i64,
) -> Result<Option<StandardRevision>, BackendError> {
    let row=sqlx::query("SELECT payload,state,state_version,withdrawn_by,withdrawn_at,withdrawal_reason FROM standard_revision WHERE tenant_id=$1 AND kind=$2 AND id=$3 AND revision_version=$4 FOR SHARE").bind(tenant).bind(kind).bind(id).bind(version).fetch_optional(conn).await.map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let mut revision: StandardRevision =
        decode(&row.try_get::<String, _>("payload").map_err(error)?)?;
    revision.state = row.try_get("state").map_err(error)?;
    revision.state_version = row.try_get("state_version").map_err(error)?;
    revision.withdrawn_by = row.try_get("withdrawn_by").map_err(error)?;
    revision.withdrawal_reason = row.try_get("withdrawal_reason").map_err(error)?;
    revision.withdrawn_at = row.try_get("withdrawn_at").map_err(error)?;
    if revision.tenant_id != tenant
        || revision.kind != kind
        || revision.id != id
        || revision.revision_version != version
        || revision.definition_hash
            != revision_hash(
                kind,
                &revision.code,
                &revision.name,
                &revision.properties,
                &revision.children,
            )?
    {
        return Err(BackendError::Internal(
            "不可变标准修订身份或hash无效".into(),
        ));
    }
    Ok(Some(revision))
}
pub(super) async fn binding(
    conn: &mut PgConnection,
    tenant: &str,
    model: &str,
) -> Result<Option<ModelStandardBindingSet>, BackendError> {
    let value:Option<String>=sqlx::query_scalar("SELECT b.payload FROM standard_model_current c JOIN standard_model_binding b ON b.tenant_id=c.tenant_id AND b.model_id=c.model_id AND b.binding_version=c.binding_version WHERE c.tenant_id=$1 AND c.model_id=$2 FOR SHARE OF c,b").bind(tenant).bind(model).fetch_optional(conn).await.map_err(error)?;
    let binding = value
        .as_deref()
        .map(decode::<ModelStandardBindingSet>)
        .transpose()?;
    if let Some(binding) = &binding {
        if binding.tenant_id != tenant
            || binding.model_id != model
            || binding.definition_hash
                != binding_hash(
                    binding.coverage,
                    &binding.columns,
                    &binding.table_naming,
                    &binding.model_properties_hash,
                )?
        {
            return Err(BackendError::Internal(
                "不可变模型标准绑定身份或hash无效".into(),
            ));
        }
    }
    Ok(binding)
}
