// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{
    BackendError,
    repo::{metadata_catalog::*, product_resource::ProductResource},
};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool, Row};

use super::super::product_resource::PgProductResourceRepo;

pub(super) fn error(error: sqlx::Error) -> BackendError {
    super::super::product_resource::map_reference_error(error)
}

pub(super) async fn guard(
    conn: &mut PgConnection,
    params: &MetadataApplyParams,
) -> Result<(), BackendError> {
    let actor: Option<String> = sqlx::query_scalar("SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id
        WHERE u.id=$1 AND t.id=$2 AND u.status='active' AND t.status='active'
        AND EXISTS(SELECT 1 FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id
            WHERE ur.user_id=u.id AND r.tenant_id=t.id AND rp.permission='metadata:execute')
        AND EXISTS(SELECT 1 FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id
            WHERE ur.user_id=u.id AND r.tenant_id=t.id AND rp.permission='metadata:read') FOR SHARE OF u,t")
        .bind(&params.actor_id).bind(&params.tenant_id).fetch_optional(&mut *conn).await.map_err(error)?;
    if actor.is_none() {
        return Err(BackendError::ConstraintViolation(
            "采集操作人或精确权限已失效".into(),
        ));
    }
    let permissions: Vec<String> = sqlx::query_scalar(
        "SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id
        JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=$1 AND r.tenant_id=$2
        AND rp.permission IN ('metadata:read','metadata:execute') FOR SHARE OF ur,r,rp",
    )
    .bind(&params.actor_id)
    .bind(&params.tenant_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(error)?;
    if !["metadata:read", "metadata:execute"]
        .iter()
        .all(|permission| permissions.iter().any(|actual| actual == permission))
    {
        return Err(BackendError::ConstraintViolation(
            "采集精确权限在事务中已撤回".into(),
        ));
    }
    let source = PgProductResourceRepo::read_resource(
        conn,
        &params.tenant_id,
        "metadata_collection",
        &params.collection_id,
    )
    .await?
    .ok_or_else(|| BackendError::NotFound("采集定义不存在".into()))?;
    if source.version != params.collection_version
        || source.properties != params.collection_properties
        || !["draft", "published"].contains(&source.status.as_str())
    {
        return Err(BackendError::Conflict("采集定义已改变或撤回".into()));
    }
    let snapshot: Option<String> = sqlx::query_scalar("SELECT j.definition_snapshot::text FROM product_job j
        JOIN product_job_node n ON n.tenant_id=j.tenant_id AND n.job_id=j.id
        WHERE j.tenant_id=$1 AND j.id=$2 AND j.source_kind='metadata_collection' AND j.source_id=$3
        AND j.source_version=$4 AND j.actor_id=$5 AND j.action='collect_metadata' AND j.can_retry=TRUE
        AND j.status='running' AND j.cancel_requested=FALSE AND j.lease_owner=$6 AND j.lease_fence=$7
        AND j.lease_expires_at>clock_timestamp() AND n.node_id=$8 AND n.status='running' FOR UPDATE OF j,n")
        .bind(&params.tenant_id).bind(&params.job_id).bind(&params.collection_id).bind(params.collection_version)
        .bind(&params.actor_id).bind(&params.lease.worker_id).bind(params.lease.fence).bind(&params.node_id)
        .fetch_optional(&mut *conn).await.map_err(error)?;
    let snapshot: Value =
        serde_json::from_str(&snapshot.ok_or_else(|| {
            BackendError::Conflict("采集运行已取消、租约失效或节点状态改变".into())
        })?)
        .map_err(|_| BackendError::Internal("采集运行快照无效".into()))?;
    if snapshot["executor"] != "metadata_catalog_v1"
        || snapshot["format_version"] != 1
        || snapshot["properties"] != params.collection_properties
        || snapshot["scope"] != json!(params.scope)
    {
        return Err(BackendError::Conflict(
            "采集范围与固定运行快照不一致".into(),
        ));
    }
    Ok(())
}

pub(super) async fn audit(
    conn: &mut PgConnection,
    params: &MetadataApplyParams,
    kind: &str,
    id: &str,
    operation: &str,
    summary: Value,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO product_resource_audit(id,tenant_id,actor_id,resource_kind,resource_id,operation,summary,created_at)
        VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,CURRENT_TIMESTAMP)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&params.tenant_id).bind(&params.actor_id).bind(kind).bind(id)
        .bind(operation).bind(summary.to_string()).execute(conn).await.map_err(error)?;
    Ok(())
}

pub(super) async fn resource(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
) -> Result<ProductResource, BackendError> {
    PgProductResourceRepo::read_resource(conn, tenant, "metadata_object", id)
        .await?
        .ok_or_else(|| BackendError::Conflict("内部目录关联元数据对象已不存在".into()))
}

pub(super) async fn verified(
    pool: &PgPool,
    tenant: &str,
    id: &str,
    version: i64,
) -> Result<Option<MetadataEngineBinding>, BackendError> {
    let mut tx = pool.begin().await.map_err(error)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(error)?;
    let row=sqlx::query("SELECT c.*, c.columns_snapshot::text AS encoded_columns FROM metadata_catalog c JOIN product_resource r
        ON r.tenant_id=c.tenant_id AND r.kind=c.object_kind AND r.id=c.object_id
        WHERE c.tenant_id=$1 AND c.object_id=$2 AND c.object_version=$3 AND r.version=$3
        AND c.engine='swanlake' AND c.discovery_status='present' AND r.status='published'")
        .bind(tenant).bind(id).bind(version).fetch_optional(&mut *tx).await.map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let table = MetadataTable {
        table_catalog: row.try_get("table_catalog").map_err(error)?,
        schema_name: row.try_get("schema_name").map_err(error)?,
        table_name: row.try_get("table_name").map_err(error)?,
        table_type: row.try_get("table_type").map_err(error)?,
        columns: serde_json::from_str(&row.try_get::<String, _>("encoded_columns").map_err(error)?)
            .map_err(|_| BackendError::Internal("内部字段快照无效".into()))?,
    };
    let binding = MetadataEngineBinding {
        engine: row.try_get("engine").map_err(error)?,
        tenant_id: tenant.into(),
        object_version: version,
        table_catalog: table.table_catalog.clone(),
        schema_name: table.schema_name.clone(),
        table_name: table.table_name.clone(),
        source_fingerprint: row.try_get("source_fingerprint").map_err(error)?,
    };
    let resource = resource(&mut tx, tenant, id).await?;
    let valid = binding_matches(&binding, &resource.properties)
        && resource.properties["columns"] == json!(table.columns)
        && binding.source_fingerprint == table.fingerprint()?;
    tx.commit().await.map_err(error)?;
    Ok(valid.then_some(binding))
}
