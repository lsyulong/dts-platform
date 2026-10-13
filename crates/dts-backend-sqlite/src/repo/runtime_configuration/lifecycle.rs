use super::{SqliteRuntimeConfigurationRepo, storage::*};
use dts_backend::{BackendError, repo::runtime_configuration::*};
use serde_json::Value;
use sqlx::Row;

pub(super) async fn change(
    repo: &SqliteRuntimeConfigurationRepo,
    lookup: &RuntimeConfigurationLookup,
    expected: i64,
    reason: &str,
    key: &str,
    witness: Option<&ValidatedRuntimePolicy>,
    publish: bool,
) -> Result<RuntimeConfigurationChange, BackendError> {
    validation::request(expected, reason, key)?;
    validation::text(&lookup.configuration_id, 128)?;
    let action = if publish { "publish" } else { "withdraw" };
    let semantic = validation::semantic(&lookup.configuration_id, expected, reason);
    let mut tx = repo.write(&lookup.scope.tenant_id).await?;
    authorize(&mut tx, &lookup.scope, RuntimeSourceAccess::Publish).await?;
    let before = resource(
        &mut tx,
        &lookup.scope.tenant_id,
        "development_config",
        &lookup.configuration_id,
    )
    .await?;
    let replay=sqlx::query("SELECT actor_id,action,semantic,result_snapshot FROM runtime_configuration_receipt WHERE tenant_id=?1 AND idempotency_key=?2")
        .bind(&lookup.scope.tenant_id).bind(key).fetch_optional(&mut *tx).await.map_err(error)?;
    if let Some(row) = replay {
        if row.try_get::<String, _>("actor_id").map_err(error)? != lookup.scope.actor_id
            || row.try_get::<String, _>("action").map_err(error)? != action
            || decode::<Value>(&row.try_get::<String, _>("semantic").map_err(error)?)? != semantic
        {
            return Err(conflict("配置幂等键已用于不同操作或身份"));
        }
        let mut result: RuntimeConfigurationChange =
            decode(&row.try_get::<String, _>("result_snapshot").map_err(error)?)?;
        result.replayed = true;
        tx.commit().await.map_err(error)?;
        return Ok(result);
    }
    if before.version != expected {
        return Err(conflict("配置版本已变化"));
    }
    if publish {
        if !["draft", "inactive"].contains(&before.status.as_str()) {
            return Err(conflict("当前配置状态不能发布"));
        }
        let policy = validation::policy(&before.properties)?;
        let source = if let (Some(kind), Some(id)) = (&policy.source_kind, &policy.source_id) {
            Some(resource(&mut tx, &lookup.scope.tenant_id, kind, id).await?)
        } else {
            None
        };
        if witness.is_none()
            || !witness.is_some_and(|w| w.matches(&before, source.as_ref()).unwrap_or(false))
        {
            return Err(conflict("缺少配置与来源实际校验证据"));
        }
    } else if before.status != "published" {
        return Err(conflict("只能撤回已发布配置"));
    }
    let resource = status(
        &mut tx,
        &lookup.scope,
        &before,
        if publish { "published" } else { "inactive" },
        action,
    )
    .await?;
    let evidence = if publish {
        let evidence = validation::evidence(&resource)?;
        sqlx::query("INSERT INTO runtime_configuration_publication(tenant_id,configuration_id,configuration_version,definition_hash,snapshot) VALUES(?1,?2,?3,?4,?5)")
            .bind(&resource.tenant_id).bind(&resource.id).bind(resource.version).bind(&evidence.configuration_hash).bind(encode(&resource)?).execute(&mut *tx).await.map_err(error)?;
        Some(evidence)
    } else {
        None
    };
    let result = RuntimeConfigurationChange {
        resource,
        evidence,
        receipt_id: uuid::Uuid::new_v4().to_string(),
        replayed: false,
    };
    sqlx::query("INSERT INTO runtime_configuration_receipt(id,tenant_id,actor_id,idempotency_key,action,semantic,result_snapshot) VALUES(?1,?2,?3,?4,?5,?6,?7)")
        .bind(&result.receipt_id).bind(&lookup.scope.tenant_id).bind(&lookup.scope.actor_id).bind(key).bind(action).bind(encode(&semantic)?).bind(encode(&result)?).execute(&mut *tx).await.map_err(error)?;
    tx.commit().await.map_err(error)?;
    Ok(result)
}
