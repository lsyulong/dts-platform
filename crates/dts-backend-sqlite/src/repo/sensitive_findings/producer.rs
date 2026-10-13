// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{SqliteSensitiveFindingRepo, storage::*};
use dts_backend::{BackendError, repo::sensitive_findings::*};
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection};

pub(super) async fn commit(
    repo: &SqliteSensitiveFindingRepo,
    p: &SensitiveCommitParams,
) -> Result<SensitiveScanResult, BackendError> {
    validate_result(&p.snapshot, &p.result)?;
    let mut tx = repo
        .pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(error)?;
    authority(&mut tx, &p.tenant_id, &p.actor_id, "execute").await?;
    let actual=sqlx::query("SELECT definition_snapshot,source_id,source_version FROM product_job WHERE tenant_id=?1 AND id=?2 AND actor_id=?3 AND source_kind='recognition_task' AND action='execute_recognition' AND status='running' AND can_retry=1 AND cancel_requested=0 AND lease_owner=?4 AND lease_fence=?5 AND julianday(lease_expires_at)>julianday('now')").bind(&p.tenant_id).bind(&p.job_id).bind(&p.actor_id).bind(&p.lease.worker_id).bind(p.lease.fence).fetch_optional(&mut *tx).await.map_err(error)?.ok_or_else(||invalid("识别producer租约、任务来源或取消状态已失效"))?;
    let snapshot: Value = serde_json::from_str(
        &actual
            .try_get::<String, _>("definition_snapshot")
            .map_err(error)?,
    )
    .map_err(|_| invalid("真实识别快照无效"))?;
    if snapshot != p.snapshot
        || snapshot["tenant_id"] != p.tenant_id
        || snapshot["source_id"] != actual.try_get::<String, _>("source_id").map_err(error)?
        || snapshot["source_version"]
            != actual.try_get::<i64, _>("source_version").map_err(error)?
        || p.node_id != "recognition"
    {
        return Err(invalid("识别producer与真实冻结来源不一致"));
    }
    let nodes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM product_job_node WHERE tenant_id=?1 AND job_id=?2",
    )
    .bind(&p.tenant_id)
    .bind(&p.job_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(error)?;
    let node=sqlx::query("SELECT definition,depends_on FROM product_job_node WHERE tenant_id=?1 AND job_id=?2 AND node_id=?3 AND status='running'").bind(&p.tenant_id).bind(&p.job_id).bind(&p.node_id).fetch_optional(&mut *tx).await.map_err(error)?.ok_or_else(||invalid("识别producer没有实际Running节点"))?;
    let definition: Value =
        serde_json::from_str(&node.try_get::<String, _>("definition").map_err(error)?)
            .map_err(|_| invalid("节点定义无效"))?;
    if nodes != 1
        || definition["type"] != "sensitive_scan"
        || definition["definition_hash"] != p.result.definition_hash
        || node.try_get::<String, _>("depends_on").map_err(error)? != "[]"
    {
        return Err(invalid("敏感producer只接受单个真实冻结扫描节点"));
    }
    current_sources(&mut tx, &p.tenant_id, &snapshot).await?;
    let mut result = p.result.clone();
    for evidence in result.columns.iter().filter(|c| c.matched_values > 0) {
        let id = uuid::Uuid::new_v4().to_string();
        insert_finding(&mut tx, p, &snapshot, evidence, &id).await?;
        result.finding_ids.push(id);
    }
    result.producer_committed = true;
    let encoded = serde_json::to_string(&result).map_err(|_| invalid("识别结果无法编码"))?;
    let changed=sqlx::query("UPDATE product_job_node SET status='succeeded',result=?4,error=NULL,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE tenant_id=?1 AND job_id=?2 AND node_id=?3 AND status='running'").bind(&p.tenant_id).bind(&p.job_id).bind(&p.node_id).bind(&encoded).execute(&mut *tx).await.map_err(error)?;
    if changed.rows_affected() != 1 {
        return Err(invalid("扫描节点提交发生冲突"));
    }
    let changed=sqlx::query("UPDATE product_job SET status='succeeded',result=?6,error=NULL,lease_owner=NULL,lease_expires_at=NULL,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE tenant_id=?1 AND id=?2 AND status='running' AND cancel_requested=0 AND lease_owner=?3 AND lease_fence=?4 AND actor_id=?5 AND julianday(lease_expires_at)>julianday('now')").bind(&p.tenant_id).bind(&p.job_id).bind(&p.lease.worker_id).bind(p.lease.fence).bind(&p.actor_id).bind(&encoded).execute(&mut *tx).await.map_err(error)?;
    if changed.rows_affected() != 1 {
        return Err(invalid("识别租约在真实提交前已失效"));
    }
    tx.commit().await.map_err(error)?;
    Ok(result)
}
pub(super) async fn current_sources(
    conn: &mut SqliteConnection,
    tenant: &str,
    snapshot: &Value,
) -> Result<(), BackendError> {
    for (key, kind) in [
        ("task", "recognition_task"),
        ("rule", "recognition_rule"),
        ("classification", "data_classification"),
        ("metadata", "metadata_object"),
    ] {
        let frozen = &snapshot[key];
        let id = frozen["id"]
            .as_str()
            .ok_or_else(|| invalid("冻结识别引用缺少标识"))?;
        let current = resource(conn, tenant, kind, id).await?;
        if current.status != "published"
            || Some(current.version) != frozen["version"].as_i64()
            || current.properties != frozen["properties"]
        {
            return Err(invalid("识别依赖定义或元数据已改变，不能提交"));
        }
    }
    let meta = &snapshot["metadata"];
    live_metadata(
        conn,
        tenant,
        meta["id"].as_str().ok_or_else(|| invalid("来源标识无效"))?,
        meta["version"]
            .as_i64()
            .ok_or_else(|| invalid("来源版本无效"))?,
        meta["binding"]["source_fingerprint"]
            .as_str()
            .ok_or_else(|| invalid("来源指纹无效"))?,
    )
    .await
}
async fn insert_finding(
    conn: &mut SqliteConnection,
    p: &SensitiveCommitParams,
    s: &Value,
    e: &SensitiveColumnEvidence,
    id: &str,
) -> Result<(), BackendError> {
    // The directory exposes references and review state, never raw samples or counts.
    let props = json!({"task_id":s["task"]["id"],"column_name":e.column_name,"classification_id":s["classification"]["id"],"evidence":{"producer":"sensitive_scan_v1","job_id":p.job_id,"node_id":p.node_id,"definition_hash":p.result.definition_hash,"evaluation_scope":p.result.evaluation_scope},"evidence_summary":"真实有界扫描发现；仅代表当次取样，需人工复核。计数在受控证据页读取。","confirmed":false});
    sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,status,properties,version,created_by,created_at,updated_at) VALUES(?1,?2,'sensitive_finding',?3,?4,'pending',?5,1,?6,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))").bind(id).bind(&p.tenant_id).bind(format!("finding_{id}")).bind(format!("待复核字段 {}",e.column_name)).bind(props.to_string()).bind(&p.actor_id).execute(&mut *conn).await.map_err(error)?;
    for (field, kind, key) in [
        ("task_id", "recognition_task", "task"),
        ("classification_id", "data_classification", "classification"),
    ] {
        sqlx::query("INSERT INTO product_resource_links(tenant_id,source_kind,source_id,field,target_kind,target_id) VALUES(?1,'sensitive_finding',?2,?3,?4,?5)").bind(&p.tenant_id).bind(id).bind(field).bind(kind).bind(s[key]["id"].as_str().ok_or_else(||invalid("发现来源缺少标识"))?).execute(&mut *conn).await.map_err(error)?;
    }
    sqlx::query("INSERT INTO sensitive_finding(id,tenant_id,task_id,task_version,object_id,object_version,rule_id,rule_version,classification_id,classification_version,job_id,node_id,column_name,evidence,evaluation_scope,sampled_rows,definition_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)").bind(id).bind(&p.tenant_id).bind(s["task"]["id"].as_str()).bind(s["task"]["version"].as_i64()).bind(s["metadata"]["id"].as_str()).bind(s["metadata"]["version"].as_i64()).bind(s["rule"]["id"].as_str()).bind(s["rule"]["version"].as_i64()).bind(s["classification"]["id"].as_str()).bind(s["classification"]["version"].as_i64()).bind(&p.job_id).bind(&p.node_id).bind(&e.column_name).bind(json!(e).to_string()).bind(&p.result.evaluation_scope).bind(i64::from(p.result.sampled_rows)).bind(&p.result.definition_hash).execute(&mut *conn).await.map_err(error)?;
    audit(conn, &p.tenant_id, id, &p.actor_id, "discovered", None).await?;
    resource_audit(conn,&p.tenant_id,id,&p.actor_id,"create",json!({"producer":"sensitive_scan_v1","job_id":p.job_id,"node_id":p.node_id,"status":"pending","version":1})).await
}
