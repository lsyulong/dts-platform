use super::storage::*;
use dts_backend::{BackendError, repo::quality_alerts::*};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use std::collections::BTreeSet;

pub(super) async fn observation(
    conn: &mut PgConnection,
    observation: &QualityObservation,
) -> Result<(), BackendError> {
    for proof in &observation.snapshot.resources {
        let row=sqlx::query("SELECT version,status,properties::text AS properties FROM product_resource WHERE tenant_id=$1 AND kind=$2 AND id=$3 FOR SHARE").bind(&observation.tenant_id).bind(&proof.kind).bind(&proof.id).fetch_optional(&mut *conn).await.map_err(error)?.ok_or_else(||conflict("冻结质量引用已不存在"))?;
        if proof.kind == "metadata_object"
            && row.try_get::<String, _>("status").map_err(error)? != "published"
        {
            return Err(conflict("元数据缺少实际有效采集状态"));
        }
        let properties: Value = decode(&row.try_get::<String, _>("properties").map_err(error)?)?;
        if row.try_get::<i64, _>("version").map_err(error)? != proof.version
            || properties != proof.properties
        {
            return Err(conflict("监控、任务、规则或元数据版本已改变，请重新评估"));
        }
    }
    let snapshot = &observation.snapshot;
    let row=sqlx::query("SELECT source_fingerprint,columns_snapshot::text AS columns_snapshot FROM metadata_catalog WHERE tenant_id=$1 AND object_id=$2 AND object_version=$3 AND engine='swanlake' AND discovery_status='present' FOR SHARE").bind(&observation.tenant_id).bind(&snapshot.metadata_id).bind(snapshot.metadata_version).fetch_optional(&mut *conn).await.map_err(error)?.ok_or_else(||conflict("实际元数据采集证明已经失效"))?;
    let metadata = snapshot
        .resources
        .iter()
        .find(|proof| proof.kind == "metadata_object" && proof.id == snapshot.metadata_id)
        .ok_or_else(|| conflict("缺少真实元数据冻结引用"))?;
    if row
        .try_get::<String, _>("source_fingerprint")
        .map_err(error)?
        != snapshot.source_fingerprint
        || decode::<Value>(
            &row.try_get::<String, _>("columns_snapshot")
                .map_err(error)?,
        )? != metadata.properties["columns"]
    {
        return Err(conflict("实际元数据字段或指纹已经改变"));
    }
    let current:Vec<String>=sqlx::query_scalar("SELECT id FROM product_job WHERE tenant_id=$1 AND source_kind='quality_task' AND action='execute_quality' AND definition_snapshot->'object'->>'id'=$2 AND COALESCE(finished_at,updated_at)>=$3 AND COALESCE(finished_at,updated_at)<$4 ORDER BY id LIMIT 513 FOR SHARE").bind(&observation.tenant_id).bind(&observation.object_id).bind(observation.period_start).bind(observation.period_end).fetch_all(&mut *conn).await.map_err(error)?;
    let expected: BTreeSet<_> = snapshot.jobs.iter().map(|job| job.id.clone()).collect();
    let actual: BTreeSet<_> = current.into_iter().collect();
    if (observation.summary["window_complete"] == true && actual != expected)
        || (actual.len() < 513 && !expected.is_subset(&actual))
    {
        return Err(conflict("监控窗口中的真实运行发生变化，请重试评估"));
    }
    for job in &snapshot.jobs {
        verify_job(conn, &observation.tenant_id, job).await?;
    }
    Ok(())
}

async fn verify_job(
    conn: &mut PgConnection,
    tenant: &str,
    proof: &QualityJobProof,
) -> Result<(), BackendError> {
    let row=sqlx::query("SELECT source_id,source_version,status,updated_at,COALESCE(finished_at,updated_at) AS event_time,definition_snapshot::text AS definition_snapshot FROM product_job WHERE tenant_id=$1 AND id=$2 AND source_kind='quality_task' AND action='execute_quality' FOR SHARE").bind(tenant).bind(&proof.id).fetch_optional(&mut *conn).await.map_err(error)?.ok_or_else(||conflict("真实质量运行已不存在"))?;
    let parse = |name: &str| -> Result<chrono::DateTime<chrono::Utc>, BackendError> {
        row.try_get(name).map_err(error)
    };
    if row.try_get::<String, _>("source_id").map_err(error)? != proof.source_id
        || row.try_get::<i64, _>("source_version").map_err(error)? != proof.source_version
        || row.try_get::<String, _>("status").map_err(error)? != proof.status
        || parse("updated_at")? != proof.updated_at
        || parse("event_time")? != proof.event_time
        || decode::<Value>(
            &row.try_get::<String, _>("definition_snapshot")
                .map_err(error)?,
        )? != proof.definition_snapshot
    {
        return Err(conflict("真实质量运行的状态或冻结定义已改变"));
    }
    let rows=sqlx::query("SELECT node_id,status,definition::text AS definition,result::text AS result FROM product_job_node WHERE tenant_id=$1 AND job_id=$2 ORDER BY node_id FOR SHARE").bind(tenant).bind(&proof.id).fetch_all(conn).await.map_err(error)?;
    if rows.len() != proof.nodes.len() {
        return Err(conflict("真实质量运行节点数已改变"));
    }
    for row in rows {
        let id: String = row.try_get("node_id").map_err(error)?;
        let node = proof
            .nodes
            .iter()
            .find(|node| node.id == id)
            .ok_or_else(|| conflict("质量节点身份不匹配"))?;
        let result: Option<String> = row.try_get("result").map_err(error)?;
        if row.try_get::<String, _>("status").map_err(error)? != node.status
            || decode::<Value>(&row.try_get::<String, _>("definition").map_err(error)?)?
                != node.definition
            || result.as_deref().map(decode::<Value>).transpose()? != node.result
        {
            return Err(conflict("真实质量节点结果发生变化"));
        }
    }
    Ok(())
}
