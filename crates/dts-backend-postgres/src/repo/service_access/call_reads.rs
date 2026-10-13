use dts_backend::{
    BackendError,
    repo::service_access::{validation, *},
};
use sqlx::{PgConnection, Row};

use super::{
    authority, calls,
    storage::{error, invalid},
};

pub(super) async fn list(
    conn: &mut PgConnection,
    f: &ServiceCallFilter,
) -> Result<ServiceCallPage, BackendError> {
    validation::pagination(f.limit, f.offset)?;
    let actor = authority::get(conn, &f.tenant_id, &f.actor_id, false).await?;
    if !actor.active {
        return Err(invalid("查看调用记录用户已失效"));
    }
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM(SELECT tenant_id,owner_id,application_id,api_id FROM service_call
        UNION ALL SELECT tenant_id,owner_id,application_id,api_id FROM service_call_denial) c WHERE tenant_id=$1 AND(owner_id=$2 OR $3)
        AND($4::text IS NULL OR application_id=$4) AND($5::text IS NULL OR api_id=$5)")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(actor.can_approve).bind(&f.application_id).bind(&f.api_id).fetch_one(&mut *conn).await.map_err(error)?;
    let rows=sqlx::query("SELECT * FROM(SELECT id,tenant_id,owner_id,application_id,credential_id,api_id,NULL::text AS target_hash,outcome,NULL::text AS reason_code,
        started_at,finished_at,duration_ms,row_count FROM service_call UNION ALL
        SELECT id,tenant_id,owner_id,application_id,credential_id,api_id,target_hash,'denied'::text AS outcome,reason_code,
        created_at AS started_at,created_at AS finished_at,NULL::bigint AS duration_ms,NULL::bigint AS row_count FROM service_call_denial) c
        WHERE tenant_id=$1 AND(owner_id=$2 OR $3) AND($4::text IS NULL OR application_id=$4) AND($5::text IS NULL OR api_id=$5)
        ORDER BY started_at DESC,id LIMIT $6 OFFSET $7")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(actor.can_approve).bind(&f.application_id).bind(&f.api_id).bind(i64::from(f.limit)).bind(i64::from(f.offset))
        .fetch_all(conn).await.map_err(error)?;
    let mut data = Vec::with_capacity(rows.len());
    for row in rows {
        let api_id: Option<String> = row.try_get("api_id").map_err(error)?;
        let hash: Option<String> = row.try_get("target_hash").map_err(error)?;
        let target_hash = match hash {
            Some(hash) => hash,
            None => validation::target_hash(
                api_id
                    .as_deref()
                    .ok_or_else(|| invalid("实际调用API缺失"))?,
            ),
        };
        data.push(ServiceCallAudit {
            id: row.try_get("id").map_err(error)?,
            tenant_id: row.try_get("tenant_id").map_err(error)?,
            owner_id: row.try_get("owner_id").map_err(error)?,
            application_id: row.try_get("application_id").map_err(error)?,
            credential_id: row.try_get("credential_id").map_err(error)?,
            api_id,
            target_hash,
            outcome: calls::outcome(&row.try_get::<String, _>("outcome").map_err(error)?)?,
            reason_code: row.try_get("reason_code").map_err(error)?,
            started_at: row.try_get("started_at").map_err(error)?,
            finished_at: row.try_get("finished_at").map_err(error)?,
            duration_ms: calls::unsigned(row.try_get("duration_ms").map_err(error)?)?,
            row_count: calls::unsigned(row.try_get("row_count").map_err(error)?)?,
        });
    }
    Ok(ServiceCallPage { data, total })
}
