// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
use super::storage;
use chrono::SecondsFormat;
use dts_backend::{BackendError, repo::bi_share::*};
use sqlx::SqliteConnection;
pub(super) async fn binding(
    conn: &mut SqliteConnection,
    tenant: &str,
    report: &str,
    version: i64,
    job: &str,
    node: &str,
) -> Result<(), BackendError> {
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM product_resource r JOIN product_job j ON j.tenant_id=r.tenant_id AND j.source_kind=r.kind AND j.source_id=r.id AND j.source_version=r.version JOIN product_job_node n ON n.tenant_id=j.tenant_id AND n.job_id=j.id WHERE r.tenant_id=?1 AND r.kind='bi_report' AND r.id=?2 AND r.version=?3 AND r.status='published' AND j.id=?4 AND j.status='succeeded' AND j.action='execute_bi' AND n.node_id=?5 AND n.status='succeeded' AND n.result IS NOT NULL AND json_extract(j.definition_snapshot,'$.executor')='bi_aggregate_v1' AND json_extract(j.definition_snapshot,'$.properties')=json(r.properties))")
        .bind(tenant).bind(report).bind(version).bind(job).bind(node).fetch_one(conn).await.map_err(storage::error)?;
    if !valid {
        return Err(storage::invalid(
            "分享必须绑定当前发布版本的真实成功BI节点，来源已撤回或改变",
        ));
    }
    Ok(())
}
pub(super) async fn create(
    conn: &mut SqliteConnection,
    params: &CreateBiShareParams,
) -> Result<BiShare, BackendError> {
    storage::authority(conn, &params.tenant_id, &params.owner_id, true).await?;
    storage::authority(conn, &params.tenant_id, &params.recipient_id, false).await?;
    binding(
        conn,
        &params.tenant_id,
        &params.report_id,
        params.report_version,
        &params.job_id,
        &params.node_id,
    )
    .await?;
    let expiry = params
        .expires_at
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    let valid: bool = sqlx::query_scalar(
        "SELECT (julianday(?1)-julianday('now')) BETWEEN (60.0/86400.0) AND 30.0",
    )
    .bind(&expiry)
    .fetch_one(&mut *conn)
    .await
    .map_err(storage::error)?;
    if !valid {
        return Err(storage::invalid(
            "分享到期时间必须在数据库当前时间60秒至30天内",
        ));
    }
    sqlx::query("INSERT INTO bi_share(id,tenant_id,report_id,report_version,job_id,node_id,owner_id,recipient_id,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)")
        .bind(&params.id).bind(&params.tenant_id).bind(&params.report_id).bind(params.report_version).bind(&params.job_id).bind(&params.node_id).bind(&params.owner_id).bind(&params.recipient_id).bind(expiry)
        .execute(&mut *conn).await.map_err(storage::error)?;
    let value = storage::required(
        conn,
        &BiShareLookup {
            tenant_id: params.tenant_id.clone(),
            actor_id: params.owner_id.clone(),
            share_id: params.id.clone(),
        },
    )
    .await?;
    storage::audit(conn, &value, &params.owner_id, "create").await?;
    Ok(value)
}
