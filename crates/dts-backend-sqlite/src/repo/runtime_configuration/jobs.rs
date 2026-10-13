use super::storage::*;
use dts_backend::{
    BackendError,
    repo::{product_job::EnqueueProductJobParams, runtime_configuration::*},
};
use sqlx::SqliteConnection;

pub(crate) async fn verify_enqueue(
    conn: &mut SqliteConnection,
    p: &EnqueueProductJobParams,
) -> Result<Option<RuntimeConfigurationEvidence>, BackendError> {
    if !matches!(
        p.source_kind.as_str(),
        "sql_document" | "development_workflow"
    ) {
        return Ok(None);
    }
    let frozen: Option<RuntimeConfigurationEvidence> = serde_json::from_value(
        p.definition_snapshot
            .get("runtime_configuration")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    )
    .map_err(|_| validation::invalid("运行配置快照无效"))?;
    let lookup = RuntimeSourceLookup {
        scope: RuntimeConfigurationScope {
            tenant_id: p.tenant_id.clone(),
            actor_id: p.actor_id.clone(),
        },
        source_kind: p.source_kind.clone(),
        source_id: p.source_id.clone(),
        source_version: p.source_version,
        access: RuntimeSourceAccess::Execute,
    };
    // Existing unconfigured jobs retain their old repository contract. A declared
    // configuration always requires the real publication and current authority.
    let actual = resource(conn, &p.tenant_id, &p.source_kind, &p.source_id).await?;
    if validation::configuration_id(&actual)?.is_none() && frozen.is_none() {
        return Ok(None);
    }
    let current = source(conn, &lookup, true).await?;
    if current.evidence != frozen {
        return Err(conflict("运行配置与真实已发布来源不一致"));
    }
    Ok(frozen)
}

pub(crate) async fn register_job(
    conn: &mut SqliteConnection,
    tenant: &str,
    job: &str,
    evidence: Option<&RuntimeConfigurationEvidence>,
) -> Result<(), BackendError> {
    if let Some(e) = evidence {
        sqlx::query("INSERT INTO runtime_job_configuration_binding(tenant_id,job_id,configuration_id,configuration_version,concurrency,evidence) VALUES(?1,?2,?3,?4,?5,?6)")
            .bind(tenant).bind(job).bind(&e.configuration_id).bind(e.configuration_version).bind(i64::from(e.concurrency)).bind(encode(e)?).execute(conn).await.map_err(error)?;
    }
    Ok(())
}

pub(crate) const CLAIM_CAPACITY: &str = "AND (NOT EXISTS (SELECT 1 FROM runtime_job_configuration_binding b WHERE b.tenant_id=product_job.tenant_id AND b.job_id=product_job.id) OR EXISTS (SELECT 1 FROM runtime_job_configuration_binding b WHERE b.tenant_id=product_job.tenant_id AND b.job_id=product_job.id AND (SELECT COUNT(*) FROM runtime_job_configuration_binding active_binding JOIN product_job active_job ON active_job.tenant_id=active_binding.tenant_id AND active_job.id=active_binding.job_id WHERE active_binding.tenant_id=b.tenant_id AND active_binding.configuration_id=b.configuration_id AND active_job.status='running' AND julianday(active_job.lease_expires_at)>julianday('now'))<b.concurrency))";
