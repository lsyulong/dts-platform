use super::{SqliteDataAccessRepo, authority, storage};
use chrono::Duration;
use dts_backend::{
    BackendError, ProductResource, ResourcePage, ResourceReference, repo::data_access::*,
};
use serde_json::json;
use sqlx::{Row, SqliteConnection};

pub(super) async fn save(
    repo: &SqliteDataAccessRepo,
    params: &SaveDataAccessRequestParams,
) -> Result<ProductResource, BackendError> {
    validation::save(params)?;
    let encoded =
        serde_json::to_string(&params.target).map_err(|_| storage::invalid("授权快照无法编码"))?;
    if encoded.len() > 1024 * 1024 {
        return Err(storage::invalid("授权目标快照超过1MiB"));
    }
    let mut tx = repo.begin(&params.tenant_id, &params.actor_id).await?;
    let previous = storage::resource(&mut tx, &params.tenant_id, &params.id).await?;
    match (&previous, params.expected_version) {
        (None, None) => {},
        (Some(resource), Some(version))
            if resource.version == version
                && resource.created_by == params.actor_id
                && ["draft", "rejected"].contains(&resource.status.as_str()) => {},
        _ => {
            return Err(storage::conflict(
                "申请版本已改变、不是本人申请或当前状态禁止编辑",
            ));
        },
    }
    if !storage::verified(&mut tx, &params.tenant_id, &params.target).await? {
        return Err(storage::conflict("元数据来源、完整字段快照或版本已经改变"));
    }
    let now = storage::now(&mut tx).await?;
    if params.expires_at <= now || params.expires_at > now + Duration::days(365) {
        return Err(storage::invalid("申请失效时间超出允许窗口"));
    }
    let resource = ProductResource {
        id: params.id.clone(),
        tenant_id: params.tenant_id.clone(),
        kind: storage::KIND.into(),
        code: params.code.clone(),
        name: params.name.clone(),
        description: params.description.clone(),
        parent_id: None,
        status: "draft".into(),
        properties: validation::request_properties(params),
        version: previous.as_ref().map_or(1, |resource| resource.version + 1),
        created_by: params.actor_id.clone(),
        created_at: previous
            .as_ref()
            .map_or(now, |resource| resource.created_at),
        updated_at: now,
        references: vec![
            ResourceReference {
                field: "object_id".into(),
                target_kind: "metadata_object".into(),
                target_id: params.target.metadata_id.clone(),
            },
            ResourceReference {
                field: "requester_id".into(),
                target_kind: "user".into(),
                target_id: params.actor_id.clone(),
            },
        ],
    };
    let saved = storage::write(&mut tx, &params.actor_id, previous.as_ref(), &resource).await?;
    sqlx::query("INSERT INTO data_access_request(tenant_id,request_id,requester_id,metadata_id,target_snapshot,requested_columns,purpose,expires_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(tenant_id,request_id) DO UPDATE SET metadata_id=excluded.metadata_id,target_snapshot=excluded.target_snapshot,
        requested_columns=excluded.requested_columns,purpose=excluded.purpose,expires_at=excluded.expires_at")
        .bind(&params.tenant_id).bind(&params.id).bind(&params.actor_id).bind(&params.target.metadata_id).bind(encoded)
        .bind(json!(params.requested_columns).to_string()).bind(&params.purpose).bind(params.expires_at.to_rfc3339())
        .execute(&mut *tx).await.map_err(storage::error)?;
    tx.commit().await.map_err(storage::error)?;
    Ok(saved)
}

pub(super) async fn get(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
) -> Result<Option<DataAccessRequest>, BackendError> {
    let Some(resource) = storage::resource(conn, tenant, id).await? else {
        return Ok(None);
    };
    let row=sqlx::query("SELECT requester_id,target_snapshot,requested_columns,purpose,expires_at FROM data_access_request WHERE tenant_id=?1 AND request_id=?2")
        .bind(tenant).bind(id).fetch_optional(conn).await.map_err(storage::error)?.ok_or_else(||storage::conflict("申请缺少真实授权记录"))?;
    let request = DataAccessRequest {
        resource,
        requester_id: row.try_get("requester_id").map_err(storage::error)?,
        target: serde_json::from_str(
            &row.try_get::<String, _>("target_snapshot")
                .map_err(storage::error)?,
        )
        .map_err(|_| storage::invalid("授权目标快照无效"))?,
        requested_columns: serde_json::from_str(
            &row.try_get::<String, _>("requested_columns")
                .map_err(storage::error)?,
        )
        .map_err(|_| storage::invalid("申请字段快照无效"))?,
        purpose: row.try_get("purpose").map_err(storage::error)?,
        expires_at: storage::time(&row, "expires_at")?,
    };
    if request.resource.created_by != request.requester_id
        || request.resource.properties["requester_id"] != request.requester_id
        || request.resource.properties["object_id"] != request.target.metadata_id
        || request.resource.properties["columns"] != json!(request.requested_columns)
        || request.resource.properties["purpose"] != request.purpose
        || request.resource.properties["metadata_version"] != request.target.binding.object_version
        || request.resource.properties["expires_at"] != json!(request.expires_at)
    {
        return Err(storage::conflict("申请真实状态与冻结快照不一致"));
    }
    Ok(Some(request))
}

pub(super) async fn list(
    repo: &SqliteDataAccessRepo,
    filter: &DataAccessRequestFilter,
) -> Result<ResourcePage, BackendError> {
    let mut tx = repo.read_begin().await?;
    let authority = authority::get(&mut tx, &filter.tenant_id, &filter.actor_id, false).await?;
    if !authority.active || (filter.approvals && !authority.can_approve) {
        return Err(storage::invalid("没有查看申请队列的权限"));
    }
    let owner = if filter.approvals {
        None
    } else {
        Some(&filter.actor_id)
    };
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM product_resource r JOIN data_access_request a ON a.tenant_id=r.tenant_id AND a.request_id=r.id
        WHERE r.tenant_id=?1 AND r.kind='data_access_request' AND (?2 IS NULL OR a.requester_id=?2) AND (?3 IS NULL OR r.status=?3)")
        .bind(&filter.tenant_id).bind(owner).bind(&filter.status).fetch_one(&mut *tx).await.map_err(storage::error)?;
    let ids:Vec<String>=sqlx::query_scalar("SELECT r.id FROM product_resource r JOIN data_access_request a ON a.tenant_id=r.tenant_id AND a.request_id=r.id
        WHERE r.tenant_id=?1 AND r.kind='data_access_request' AND (?2 IS NULL OR a.requester_id=?2) AND (?3 IS NULL OR r.status=?3)
        ORDER BY r.updated_at DESC,r.id LIMIT ?4 OFFSET ?5")
        .bind(&filter.tenant_id).bind(owner).bind(&filter.status).bind(i64::from(filter.limit.clamp(1,100))).bind(i64::from(filter.offset))
        .fetch_all(&mut *tx).await.map_err(storage::error)?;
    let mut data = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(request) = get(&mut tx, &filter.tenant_id, &id).await? {
            data.push(request.resource);
        }
    }
    tx.commit().await.map_err(storage::error)?;
    Ok(ResourcePage { data, total })
}
