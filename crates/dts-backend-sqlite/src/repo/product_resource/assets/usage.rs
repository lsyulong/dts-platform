use std::collections::{BTreeSet, HashSet};

use dts_backend::{
    BackendError,
    repo::assets::{validation::*, *},
};
use sqlx::{Row, SqliteConnection};

use super::{super::helpers::map_error, authority, proof, storage};

pub(super) async fn analyze(
    conn: &mut SqliteConnection,
    params: &AssetUsageParams,
    permissions: &BTreeSet<String>,
) -> Result<AssetUsage, BackendError> {
    usage(&params.request)?;
    let asset = storage::get(
        conn,
        &params.tenant_id,
        "asset_entry",
        &params.request.asset_id,
    )
    .await?;
    let kind = text(&asset.properties, "object_kind")?.to_owned();
    authority::require(
        permissions,
        source_permission(&kind).ok_or_else(|| invalid("资产来源类型无效"))?,
    )?;
    let view = proof::view(conn, asset).await?;
    if !view.source_valid {
        return Err(BackendError::Conflict(
            "资产来源失效或发布快照过期，请先核对".into(),
        ));
    }
    let mut result = AssetUsage {
        asset_id: params.request.asset_id.clone(),
        metadata_id: None,
        period_start: params.request.period_start,
        period_end: params.request.period_end,
        instrumentation: "not_instrumented".into(),
        scope: "recorded_controlled_query_events_only".into(),
        complete: false,
        limit: 5000,
        matched_recorded_events: None,
        counted_recorded_events: 0,
        succeeded: None,
        failed: None,
        denied: None,
        unique_actors: None,
    };
    if kind != "metadata_object" {
        return Ok(result);
    }
    let metadata_id = text(&view.resource.properties, "object_id")?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM data_access_audit a WHERE a.tenant_id=?1 AND a.action='query'
        AND julianday(a.created_at)>=julianday(?2) AND julianday(a.created_at)<julianday(?3)
        AND EXISTS(SELECT 1 FROM json_each(a.evidence,'$.metadata_ids') j WHERE j.type='text' AND j.value=?4)")
        .bind(&params.tenant_id).bind(params.request.period_start.to_rfc3339()).bind(params.request.period_end.to_rfc3339()).bind(metadata_id)
        .fetch_one(&mut *conn).await.map_err(map_error)?;
    result.metadata_id = Some(metadata_id.into());
    result.instrumentation = "data_access_audit".into();
    result.matched_recorded_events = Some(total);
    // A capped window is never presented as complete usage or full-window counts.
    if total > i64::from(result.limit) {
        return Ok(result);
    }
    let rows = sqlx::query("SELECT a.actor_id,a.outcome FROM data_access_audit a WHERE a.tenant_id=?1 AND a.action='query'
        AND julianday(a.created_at)>=julianday(?2) AND julianday(a.created_at)<julianday(?3)
        AND EXISTS(SELECT 1 FROM json_each(a.evidence,'$.metadata_ids') j WHERE j.type='text' AND j.value=?4)
        ORDER BY a.created_at,a.id LIMIT 5000")
        .bind(&params.tenant_id).bind(params.request.period_start.to_rfc3339()).bind(params.request.period_end.to_rfc3339()).bind(metadata_id)
        .fetch_all(conn).await.map_err(map_error)?;
    let mut actors = HashSet::new();
    let (mut succeeded, mut failed, mut denied) = (0, 0, 0);
    for row in &rows {
        actors.insert(row.try_get::<String, _>("actor_id").map_err(map_error)?);
        match row
            .try_get::<String, _>("outcome")
            .map_err(map_error)?
            .as_str()
        {
            "succeeded" => succeeded += 1,
            "failed" => failed += 1,
            "denied" => denied += 1,
            _ => return Err(invalid("访问审计结果无效")),
        }
    }
    result.complete = true;
    result.counted_recorded_events = rows.len() as u32;
    result.succeeded = Some(succeeded);
    result.failed = Some(failed);
    result.denied = Some(denied);
    result.unique_actors = Some(actors.len() as u32);
    Ok(result)
}
