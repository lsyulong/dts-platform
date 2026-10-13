use super::super::product_resource::SqliteProductResourceRepo;
pub(super) use super::super::product_resource::helpers::map_error as error;
use dts_backend::{BackendError, ProductResource, repo::runtime_configuration::*};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
use sqlx::{Row, SqliteConnection};

pub(super) fn encode<T: Serialize>(value: &T) -> Result<String, BackendError> {
    serde_json::to_string(value).map_err(|_| BackendError::Internal("运行配置证据编码失败".into()))
}
pub(super) fn decode<T: DeserializeOwned>(text: &str) -> Result<T, BackendError> {
    serde_json::from_str(text).map_err(|_| BackendError::Internal("运行配置持久化证据无效".into()))
}
pub(super) fn conflict(message: &str) -> BackendError {
    BackendError::Conflict(message.into())
}

pub(super) async fn authorize(
    conn: &mut SqliteConnection,
    scope: &RuntimeConfigurationScope,
    access: RuntimeSourceAccess,
) -> Result<(), BackendError> {
    validation::scope(scope)?;
    let active: Option<String> = sqlx::query_scalar("SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE u.id=?1 AND t.id=?2 AND u.status='active' AND t.status='active'")
        .bind(&scope.actor_id).bind(&scope.tenant_id).fetch_optional(&mut *conn).await.map_err(error)?;
    if active.is_none() {
        return Err(BackendError::Forbidden(
            "当前运行配置身份或租户已失效".into(),
        ));
    }
    let actual: Vec<String> = sqlx::query_scalar("SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=?1 AND r.tenant_id=?2")
        .bind(&scope.actor_id).bind(&scope.tenant_id).fetch_all(&mut *conn).await.map_err(error)?;
    let extra = match access {
        RuntimeSourceAccess::Read => None,
        RuntimeSourceAccess::Publish => Some("development:write"),
        RuntimeSourceAccess::Execute => Some("development:execute"),
    };
    if !actual.iter().any(|p| p == "development:read")
        || extra.is_some_and(|p| !actual.iter().any(|v| v == p))
    {
        return Err(BackendError::Forbidden(
            "缺少实际开发配置读取或操作权限".into(),
        ));
    }
    Ok(())
}

pub(super) async fn resource(
    conn: &mut SqliteConnection,
    tenant: &str,
    kind: &str,
    id: &str,
) -> Result<ProductResource, BackendError> {
    validation::text(id, 128)?;
    SqliteProductResourceRepo::read_resource(conn, tenant, kind, id)
        .await?
        .ok_or_else(|| BackendError::NotFound("当前租户没有此配置或来源".into()))
}

pub(super) async fn status(
    conn: &mut SqliteConnection,
    scope: &RuntimeConfigurationScope,
    before: &ProductResource,
    state: &str,
    action: &str,
) -> Result<ProductResource, BackendError> {
    if before.version == i64::MAX {
        return Err(conflict("配置或来源版本已达上限"));
    }
    let changed = sqlx::query("UPDATE product_resource SET status=?1,version=version+1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE tenant_id=?2 AND kind=?3 AND id=?4 AND version=?5")
        .bind(state).bind(&scope.tenant_id).bind(&before.kind).bind(&before.id).bind(before.version).execute(&mut *conn).await.map_err(error)?.rows_affected();
    if changed != 1 {
        return Err(conflict("配置或来源已变化"));
    }
    sqlx::query("INSERT INTO product_resource_audit(id,tenant_id,actor_id,resource_kind,resource_id,operation,summary,created_at) VALUES(?1,?2,?3,?4,?5,'update',?6,strftime('%Y-%m-%dT%H:%M:%fZ','now'))")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&scope.tenant_id).bind(&scope.actor_id).bind(&before.kind).bind(&before.id)
        .bind(json!({"action":action,"before_version":before.version,"after_version":before.version+1,"status":state}).to_string()).execute(&mut *conn).await.map_err(error)?;
    resource(conn, &scope.tenant_id, &before.kind, &before.id).await
}

pub(super) async fn source(
    conn: &mut SqliteConnection,
    lookup: &RuntimeSourceLookup,
    verify_published: bool,
) -> Result<RuntimeSourceInput, BackendError> {
    validation::source(lookup)?;
    authorize(conn, &lookup.scope, lookup.access).await?;
    let source = resource(
        conn,
        &lookup.scope.tenant_id,
        &lookup.source_kind,
        &lookup.source_id,
    )
    .await?;
    if source.version != lookup.source_version {
        return Err(conflict("实际开发来源版本已改变"));
    }
    let Some(id) = validation::configuration_id(&source)? else {
        return Ok(RuntimeSourceInput {
            source,
            configuration: None,
            evidence: None,
        });
    };
    let configuration = resource(conn, &lookup.scope.tenant_id, "development_config", &id).await?;
    let evidence = validation::evidence(&configuration)?;
    validation::matches_source(&evidence, &source)?;
    let published: Option<String> = sqlx::query_scalar("SELECT definition_hash FROM runtime_configuration_publication WHERE tenant_id=?1 AND configuration_id=?2 AND configuration_version=?3")
        .bind(&lookup.scope.tenant_id).bind(&id).bind(configuration.version).fetch_optional(&mut *conn).await.map_err(error)?;
    if published.as_deref() != Some(evidence.configuration_hash.as_str()) {
        return Err(conflict("配置未通过实际发布事务或冻结证据不一致"));
    }
    if verify_published {
        if source.status != "published" {
            return Err(conflict("开发来源未发布或已撤回"));
        }
        let binding=sqlx::query("SELECT source_hash,evidence FROM runtime_source_configuration_binding WHERE tenant_id=?1 AND source_kind=?2 AND source_id=?3 AND source_version=?4")
            .bind(&lookup.scope.tenant_id).bind(&source.kind).bind(&source.id).bind(source.version).fetch_optional(&mut *conn).await.map_err(error)?.ok_or_else(||conflict("开发来源缺少实际配置发布绑定"))?;
        if binding.try_get::<String, _>("source_hash").map_err(error)?
            != validation::resource_hash(&source)?
            || decode::<RuntimeConfigurationEvidence>(
                &binding.try_get::<String, _>("evidence").map_err(error)?,
            )? != evidence
        {
            return Err(conflict("来源或配置定义与实际发布绑定不一致"));
        }
    }
    Ok(RuntimeSourceInput {
        source,
        configuration: Some(configuration),
        evidence: Some(evidence),
    })
}
