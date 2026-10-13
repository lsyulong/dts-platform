use std::collections::BTreeSet;

use dts_backend::{
    BackendError,
    repo::assets::{validation::invalid, *},
};
use sqlx::SqliteConnection;

use super::{super::helpers::map_error, proof, storage};

pub(super) async fn list(
    conn: &mut SqliteConnection,
    params: &AssetListParams,
    permissions: &BTreeSet<String>,
) -> Result<AssetPage, BackendError> {
    let f = &params.filter;
    if f.kind != "asset_entry" {
        return Err(invalid("资产目录只列实际asset_entry"));
    }
    let allowed: Vec<&str> = ASSET_SOURCE_KINDS
        .iter()
        .copied()
        .filter(|k| source_permission(k).is_some_and(|p| permissions.contains(p)))
        .collect();
    let encoded =
        serde_json::to_string(&allowed).map_err(|e| BackendError::Internal(e.to_string()))?;
    let q = f.q.as_ref().map(|v| {
        format!(
            "%{}%",
            v.to_lowercase()
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM product_resource r WHERE r.tenant_id=?1 AND r.kind='asset_entry'
        AND (?2 IS NULL OR r.status=?2) AND (?3 IS NULL OR r.created_by=?3)
        AND (?4 IS NULL OR lower(r.name) LIKE ?4 ESCAPE '\\' OR lower(r.code) LIKE ?4 ESCAPE '\\')
        AND json_extract(r.properties,'$.object_kind') IN(SELECT value FROM json_each(?5))",
    )
    .bind(&f.tenant_id)
    .bind(&f.status)
    .bind(&f.created_by)
    .bind(&q)
    .bind(&encoded)
    .fetch_one(&mut *conn)
    .await
    .map_err(map_error)?;
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT r.id FROM product_resource r WHERE r.tenant_id=?1 AND r.kind='asset_entry'
        AND (?2 IS NULL OR r.status=?2) AND (?3 IS NULL OR r.created_by=?3)
        AND (?4 IS NULL OR lower(r.name) LIKE ?4 ESCAPE '\\' OR lower(r.code) LIKE ?4 ESCAPE '\\')
        AND json_extract(r.properties,'$.object_kind') IN(SELECT value FROM json_each(?5))
        ORDER BY r.updated_at DESC,r.id LIMIT ?6 OFFSET ?7",
    )
    .bind(&f.tenant_id)
    .bind(&f.status)
    .bind(&f.created_by)
    .bind(&q)
    .bind(encoded)
    .bind(i64::from(f.limit.clamp(1, 100)))
    .bind(i64::from(f.offset))
    .fetch_all(&mut *conn)
    .await
    .map_err(map_error)?;
    let mut data = Vec::with_capacity(ids.len());
    for id in ids {
        let resource = storage::get(conn, &f.tenant_id, "asset_entry", &id).await?;
        data.push(proof::view(conn, resource).await?);
    }
    Ok(AssetPage { data, total })
}
