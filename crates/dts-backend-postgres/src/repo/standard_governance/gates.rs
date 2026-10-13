use super::physical::{publication, receipt_valid};
use super::{PgStandardGovernanceRepo, authority, dependencies, storage::*};
use dts_backend::{BackendError, ProductResource, repo::standard_governance::*};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

pub(super) async fn begin(
    repo: &PgStandardGovernanceRepo,
    p: &BeginStandardModelPublicationParams,
) -> Result<StandardModelPublicationClaimRecord, BackendError> {
    text(&p.model_id)?;
    version(p.expected_model_version)?;
    let mut tx = repo.begin_write(&p.tenant_id).await?;
    authority::require(
        &mut tx,
        &p.tenant_id,
        &p.actor_id,
        &["models:write", "models:execute"],
    )
    .await?;
    let before = resource(&mut tx, &p.tenant_id, "data_model", &p.model_id).await?;
    let stale=before.status=="publishing" && sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM product_resource WHERE tenant_id=$1 AND kind='data_model' AND id=$2 AND version=$3 AND status='publishing' AND updated_at<=clock_timestamp()-INTERVAL '10 minutes')").bind(&p.tenant_id).bind(&p.model_id).bind(before.version).fetch_one(&mut *tx).await.map_err(error)?;
    if before.version != p.expected_model_version
        || before.properties != p.expected_properties
        || (!stale
            && !["draft", "publish_failed", "publish_unknown"].contains(&before.status.as_str()))
    {
        return Err(conflict("模型定义已改变或当前状态不能发布"));
    }
    let bindings = binding(&mut tx, &p.tenant_id, &p.model_id).await?;
    let dependencies = checked_dependencies(
        &mut tx,
        &p.tenant_id,
        &before,
        &bindings,
        p.binding_version,
        p.binding_hash.as_deref(),
        &p.dependencies,
    )
    .await?;
    let standards_state = if bindings.is_some() {
        ModelStandardsState::Validated
    } else {
        ModelStandardsState::NotConfigured
    };
    if stale {
        let changed=sqlx::query("UPDATE standard_model_gate SET phase='superseded' WHERE tenant_id=$1 AND model_id=$2 AND claim_version=$3 AND phase='claimed'").bind(&p.tenant_id).bind(&p.model_id).bind(before.version).execute(&mut *tx).await.map_err(error)?.rows_affected();
        if changed != 1 {
            return Err(conflict("陈旧发布没有唯一可接管的实际门禁"));
        }
    }
    let resource = write(
        &mut tx,
        &before,
        &p.actor_id,
        before.properties.clone(),
        "publishing",
    )
    .await?;
    let claim = StandardModelPublicationClaimRecord {
        gate_id: uuid::Uuid::new_v4().to_string(),
        tenant_id: p.tenant_id.clone(),
        actor_id: p.actor_id.clone(),
        claim_version: resource.version,
        resource,
        bindings,
        dependencies,
        standards_state,
        needs_reconciliation: stale || before.status == "publish_unknown",
    };
    let payload = encode(&claim)?;
    if payload.len() > 1048576 {
        return Err(invalid("模型发布证据超过1MiB"));
    }
    sqlx::query("INSERT INTO standard_model_gate(gate_id,tenant_id,actor_id,model_id,claim_version,payload,phase) VALUES($1,$2,$3,$4,$5,$6,'claimed')").bind(&claim.gate_id).bind(&p.tenant_id).bind(&p.actor_id).bind(&p.model_id).bind(claim.claim_version).bind(payload).execute(&mut *tx).await.map_err(error)?;
    for dependency in &claim.dependencies {
        sqlx::query("INSERT INTO standard_model_gate_revision(tenant_id,gate_id,kind,id,revision_version) VALUES($1,$2,$3,$4,$5)").bind(&p.tenant_id).bind(&claim.gate_id).bind(&dependency.kind).bind(&dependency.id).bind(dependency.revision_version).execute(&mut *tx).await.map_err(error)?;
    }
    tx.commit().await.map_err(error)?;
    Ok(claim)
}
async fn checked_dependencies(
    conn: &mut PgConnection,
    tenant: &str,
    model: &ProductResource,
    bindings: &Option<ModelStandardBindingSet>,
    expected_version: Option<i64>,
    expected_hash: Option<&str>,
    provided: &[FrozenStandardRef],
) -> Result<Vec<FrozenStandardRef>, BackendError> {
    let Some(binding) = bindings else {
        if declared_bindings(&model.properties)
            || expected_version.is_some()
            || expected_hash.is_some()
            || !provided.is_empty()
        {
            return Err(conflict("模型没有实际标准配置，不能声明已验证"));
        }
        return Ok(vec![]);
    };
    if expected_version != Some(binding.binding_version)
        || expected_hash != Some(binding.definition_hash.as_str())
        || standard_hash(&model.properties)? != binding.model_properties_hash
    {
        return Err(conflict("模型标准绑定或完整定义hash已改变"));
    }
    let actual = dependencies::for_binding(conn, tenant, binding)
        .await?
        .into_iter()
        .map(|r| r.reference())
        .collect::<Vec<_>>();
    if dependencies::keys(&actual)? != dependencies::keys(provided)?
        || actual.len() != provided.len()
    {
        return Err(conflict("发布冻结依赖必须包含全部实际绑定及递归子修订"));
    }
    Ok(actual)
}
fn declared_bindings(properties: &Value) -> bool {
    [
        "naming_standard_id",
        "naming_standard_version",
        "standard_coverage",
    ]
    .iter()
    .any(|key| properties.get(key).is_some())
        || properties["columns"].as_array().is_some_and(|columns| {
            columns.iter().any(|column| {
                [
                    "standard_id",
                    "standard_version",
                    "naming_standard_id",
                    "naming_standard_version",
                ]
                .iter()
                .any(|key| column.get(key).is_some())
            })
        })
}
fn outcome_value(outcome: &StandardModelEngineOutcome) -> Value {
    match outcome {
        StandardModelEngineOutcome::Published { receipt } => {
            json!({"state":"published","receipt":receipt})
        },
        StandardModelEngineOutcome::Failed { reason } => json!({"state":"failed","reason":reason}),
        StandardModelEngineOutcome::Unknown { reason, receipt } => {
            json!({"state":"unknown","reason":reason,"receipt":receipt})
        },
    }
}
#[derive(Deserialize)]
struct StoredResult {
    resource: ProductResource,
    publication: ProductResource,
    standards_state: ModelStandardsState,
}
impl From<StoredResult> for StandardModelPublicationResult {
    fn from(v: StoredResult) -> Self {
        Self {
            resource: v.resource,
            publication: v.publication,
            standards_state: v.standards_state,
        }
    }
}

pub(super) async fn finish(
    repo: &PgStandardGovernanceRepo,
    p: &FinishStandardModelPublicationParams,
) -> Result<StandardModelPublicationResult, BackendError> {
    if uuid::Uuid::parse_str(&p.gate_id).is_err() {
        return Err(invalid("发布门禁标识必须是UUID"));
    }
    version(p.expected_claim_version)?;
    let outcome = outcome_value(&p.outcome);
    if encode(&outcome)?.len() > 524288 {
        return Err(invalid("真实引擎回执超过512KiB，须保留引擎证据并人工核实"));
    }
    if let Some(reason) = outcome.get("reason") {
        if reason.as_str().is_none_or(|v| {
            v.trim().is_empty() || v.len() > 2000 || v.chars().any(char::is_control)
        }) {
            return Err(invalid("引擎结果说明无效"));
        }
    }
    let outcome_hash = standard_hash(&outcome)?;
    let mut tx = repo.begin_write(&p.tenant_id).await?;
    let row = sqlx::query("SELECT payload,phase,outcome_hash,result FROM standard_model_gate WHERE tenant_id=$1 AND gate_id=$2 FOR UPDATE").bind(&p.tenant_id).bind(&p.gate_id).fetch_optional(&mut *tx).await.map_err(error)?.ok_or_else(|| BackendError::NotFound("发布门禁不存在".into()))?;
    let claim: StandardModelPublicationClaimRecord =
        decode(&row.try_get::<String, _>("payload").map_err(error)?)?;
    if claim.tenant_id != p.tenant_id
        || claim.gate_id != p.gate_id
        || claim.actor_id != p.actor_id
        || claim.claim_version != p.expected_claim_version
        || claim.resource.tenant_id != p.tenant_id
        || claim.resource.kind != "data_model"
        || claim.resource.version != claim.claim_version
    {
        return Err(conflict("发布回执与原操作人或冻结门禁不一致"));
    }
    if row.try_get::<String, _>("phase").map_err(error)? == "finished" {
        if row
            .try_get::<Option<String>, _>("outcome_hash")
            .map_err(error)?
            .as_deref()
            != Some(&outcome_hash)
        {
            return Err(conflict("发布门禁已完成，不能提交不同引擎结果"));
        }
        let stored: StoredResult = decode(&row.try_get::<String, _>("result").map_err(error)?)?;
        return Ok(stored.into());
    }
    let before = resource(&mut tx, &p.tenant_id, "data_model", &claim.resource.id).await?;
    let authorized = authority::allowed(
        &mut tx,
        &p.tenant_id,
        &p.actor_id,
        &["models:write", "models:execute"],
    )
    .await?;
    let current = binding(&mut tx, &p.tenant_id, &claim.resource.id).await?;
    let checked = checked_dependencies(
        &mut tx,
        &p.tenant_id,
        &before,
        &current,
        claim.bindings.as_ref().map(|v| v.binding_version),
        claim.bindings.as_ref().map(|v| v.definition_hash.as_str()),
        &claim.dependencies,
    )
    .await;
    let standards_valid = match checked {
        Ok(_) => true,
        Err(
            BackendError::Conflict(_)
            | BackendError::ConstraintViolation(_)
            | BackendError::NotFound(_),
        ) => false,
        Err(error) => return Err(error),
    };
    let unchanged = before.version == claim.claim_version
        && before.status == "publishing"
        && before.properties == claim.resource.properties;
    let valid = authorized && standards_valid && unchanged;
    let standards_state = if !valid {
        ModelStandardsState::Invalidated
    } else {
        claim.standards_state
    };
    let (mut status, receipt, mut reason) = match &p.outcome {
        StandardModelEngineOutcome::Published { receipt }
            if valid && receipt_valid(&claim.resource.properties, receipt) =>
        {
            ("published", Some(receipt.clone()), None)
        },
        StandardModelEngineOutcome::Published { receipt } => (
            "publish_unknown",
            Some(receipt.clone()),
            Some("真实物理执行已发生，但权限、标准、模型或引擎回执已失效，须人工核实".to_string()),
        ),
        StandardModelEngineOutcome::Failed { reason } => {
            ("publish_failed", None, Some(reason.clone()))
        },
        StandardModelEngineOutcome::Unknown { reason, receipt } => {
            ("publish_unknown", receipt.clone(), Some(reason.clone()))
        },
    };
    if !unchanged {
        status = "publish_unknown";
        reason = Some(format!(
            "旧发布门禁的回执已迟到，当前模型保持原状态；{}",
            reason.unwrap_or_else(|| "须核对实际物理执行证据".into())
        ));
    }
    let standards_state = if status == "publish_unknown" {
        ModelStandardsState::Invalidated
    } else {
        standards_state
    };
    let resource = if unchanged {
        write(
            &mut tx,
            &before,
            &p.actor_id,
            before.properties.clone(),
            status,
        )
        .await?
    } else {
        before
    };
    let publication = publication(&mut tx, &claim, &resource, status, json!({"model_id":claim.resource.id,"model_version":claim.claim_version,"gate_id":claim.gate_id,"receipt":receipt,"reason":reason,"standards":{"state":standards_state,"dependencies":claim.dependencies,"binding_version":claim.bindings.as_ref().map(|v|v.binding_version),"binding_hash":claim.bindings.as_ref().map(|v|v.definition_hash.clone())}})).await?;
    let result = StandardModelPublicationResult {
        resource,
        publication,
        standards_state,
    };
    let result_json = encode(&result)?;
    if result_json.len() > 1048576 {
        return Err(invalid("模型发布最终证据超过1MiB"));
    }
    let count = sqlx::query("UPDATE standard_model_gate SET phase='finished',outcome_hash=$1,result=$2 WHERE tenant_id=$3 AND gate_id=$4 AND phase IN('claimed','superseded')").bind(outcome_hash).bind(result_json).bind(&p.tenant_id).bind(&p.gate_id).execute(&mut *tx).await.map_err(error)?.rows_affected();
    if count != 1 {
        return Err(conflict("发布门禁已被其他结果完成"));
    }
    tx.commit().await.map_err(error)?;
    Ok(result)
}
