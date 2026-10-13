// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    PgMetadataCatalogRepo,
    storage::{self, error},
};
use dts_backend::{BackendError, repo::metadata_catalog::*};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};
use std::collections::BTreeSet;

pub(super) async fn apply(
    repo: &PgMetadataCatalogRepo,
    params: &MetadataApplyParams,
) -> Result<MetadataApplyResult, BackendError> {
    validate_snapshot(params)?;
    let mut tx = repo.pool.begin().await.map_err(error)?;
    let tenant: Option<String> = sqlx::query_scalar("SELECT id FROM tenant WHERE id=$1 FOR UPDATE")
        .bind(&params.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(error)?;
    if tenant.is_none() {
        return Err(BackendError::NotFound("采集租户不存在".into()));
    }
    storage::guard(&mut tx, params).await?;
    let mut result = MetadataApplyResult {
        collected_tables: params.tables.len() as u32,
        collected_columns: params
            .tables
            .iter()
            .map(|table| table.columns.len() as u32)
            .sum(),
        created_objects: 0,
        updated_objects: 0,
        unchanged_objects: 0,
        missing_objects: 0,
        scope: params.scope.clone(),
        reflection_committed: true,
    };
    for table in &params.tables {
        upsert(&mut tx, params, table, &mut result).await?;
    }
    mark_missing(&mut tx, params, &mut result).await?;
    let affected=sqlx::query("UPDATE product_job_node SET status='succeeded',result=$1,error=NULL,
        finished_at=CURRENT_TIMESTAMP WHERE tenant_id=$2 AND job_id=$3 AND node_id=$4 AND status='running'
        AND EXISTS(SELECT 1 FROM product_job j WHERE j.tenant_id=$2 AND j.id=$3 AND j.status='running'
            AND j.cancel_requested=FALSE AND j.lease_owner=$5 AND j.lease_fence=$6 AND j.lease_expires_at>clock_timestamp())")
        .bind(json!(result).to_string()).bind(&params.tenant_id).bind(&params.job_id).bind(&params.node_id)
        .bind(&params.lease.worker_id).bind(params.lease.fence).execute(&mut *tx).await.map_err(error)?.rows_affected();
    if affected != 1 {
        return Err(BackendError::Conflict(
            "采集提交时租约失效或运行已取消，快照未应用".into(),
        ));
    }
    storage::audit(&mut tx,params,"metadata_collection",&params.collection_id,"update",
        json!({"event":"metadata_reflection_committed","job_id":params.job_id,"source_version":params.collection_version,
            "collected_tables":result.collected_tables,"collected_columns":result.collected_columns,"created_objects":result.created_objects,
            "updated_objects":result.updated_objects,"missing_objects":result.missing_objects})).await?;
    tx.commit().await.map_err(error)?;
    Ok(result)
}

async fn upsert(
    conn: &mut PgConnection,
    params: &MetadataApplyParams,
    table: &MetadataTable,
    result: &mut MetadataApplyResult,
) -> Result<(), BackendError> {
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT object_id FROM metadata_catalog
        WHERE tenant_id=$1 AND engine=$2 AND table_catalog=$3 AND schema_name=$4 AND table_name=$5",
    )
    .bind(&params.tenant_id)
    .bind(&params.scope.engine)
    .bind(&table.table_catalog)
    .bind(&table.schema_name)
    .bind(&table.table_name)
    .fetch_optional(&mut *conn)
    .await
    .map_err(error)?;
    let projection = table.properties(&params.tenant_id)?;
    let (id, version) = if let Some(id) = existing {
        let resource = storage::resource(conn, &params.tenant_id, &id).await?;
        let mut properties = resource.properties.clone();
        let object = properties
            .as_object_mut()
            .ok_or_else(|| BackendError::Conflict("元数据对象属性不再是对象".into()))?;
        for (key, value) in projection
            .as_object()
            .ok_or_else(|| BackendError::Internal("元数据投影无效".into()))?
        {
            object.insert(key.clone(), value.clone());
        }
        if properties == resource.properties && resource.status == "published" {
            result.unchanged_objects += 1;
            (id, resource.version)
        } else {
            let version = resource
                .version
                .checked_add(1)
                .ok_or_else(|| BackendError::Conflict("元数据对象版本已达上限".into()))?;
            update(conn, params, &id, version, "published", &properties).await?;
            storage::audit(conn,params,"metadata_object",&id,"update",json!({"event":"metadata_observed","before_version":resource.version,"after_version":version,"job_id":params.job_id})).await?;
            result.updated_objects += 1;
            (id, version)
        }
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,status,properties,version,created_by,created_at,updated_at)
            VALUES($1,$2,'metadata_object',$3,$4,'published',$5::jsonb,1,$6,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")
            .bind(&id).bind(&params.tenant_id).bind(table.identity_code(&params.tenant_id,&params.scope.engine)?).bind(&table.table_name)
            .bind(projection.to_string()).bind(&params.actor_id).execute(&mut *conn).await.map_err(error)?;
        storage::audit(
            conn,
            params,
            "metadata_object",
            &id,
            "create",
            json!({"event":"metadata_observed","after_version":1,"job_id":params.job_id}),
        )
        .await?;
        result.created_objects += 1;
        (id, 1)
    };
    sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,
        columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9::jsonb,$10,'present',$11,CURRENT_TIMESTAMP)
        ON CONFLICT(tenant_id,engine,table_catalog,schema_name,table_name) DO UPDATE SET object_version=excluded.object_version,
        table_type=excluded.table_type,columns_snapshot=excluded.columns_snapshot,source_fingerprint=excluded.source_fingerprint,
        discovery_status='present',last_collection_id=excluded.last_collection_id,last_observed_at=excluded.last_observed_at")
        .bind(&params.tenant_id).bind(&params.scope.engine).bind(&table.table_catalog).bind(&table.schema_name).bind(&table.table_name)
        .bind(&id).bind(version).bind(&table.table_type).bind(json!(table.columns).to_string()).bind(table.fingerprint()?)
        .bind(&params.collection_id).execute(&mut *conn).await.map_err(error)?;
    Ok(())
}

async fn update(
    conn: &mut PgConnection,
    params: &MetadataApplyParams,
    id: &str,
    version: i64,
    status: &str,
    properties: &Value,
) -> Result<(), BackendError> {
    sqlx::query("UPDATE product_resource SET version=$1,status=$2,properties=$3::jsonb,updated_at=CURRENT_TIMESTAMP
        WHERE tenant_id=$4 AND kind='metadata_object' AND id=$5")
        .bind(version).bind(status).bind(properties.to_string()).bind(&params.tenant_id).bind(id).execute(conn).await.map_err(error)?;
    Ok(())
}

async fn mark_missing(
    conn: &mut PgConnection,
    params: &MetadataApplyParams,
    result: &mut MetadataApplyResult,
) -> Result<(), BackendError> {
    let present: BTreeSet<_> = params
        .tables
        .iter()
        .map(|table| (&table.table_catalog, &table.schema_name, &table.table_name))
        .collect();
    let rows=sqlx::query("SELECT object_id,table_catalog,schema_name,table_name FROM metadata_catalog
        WHERE tenant_id=$1 AND engine=$2 AND discovery_status='present' AND ($3::text IS NULL OR table_catalog=$3)
        AND ($4::text IS NULL OR schema_name=$4) AND ($5::text IS NULL OR table_name=$5)")
        .bind(&params.tenant_id).bind(&params.scope.engine).bind(&params.scope.table_catalog).bind(&params.scope.schema_name).bind(&params.scope.table_name)
        .fetch_all(&mut *conn).await.map_err(error)?;
    for row in rows {
        let catalog: String = row.try_get("table_catalog").map_err(error)?;
        let schema: String = row.try_get("schema_name").map_err(error)?;
        let table: String = row.try_get("table_name").map_err(error)?;
        if present.contains(&(&catalog, &schema, &table)) {
            continue;
        }
        let id: String = row.try_get("object_id").map_err(error)?;
        let resource = storage::resource(conn, &params.tenant_id, &id).await?;
        let version = resource
            .version
            .checked_add(1)
            .ok_or_else(|| BackendError::Conflict("元数据版本已达上限".into()))?;
        let mut properties = resource.properties;
        properties
            .as_object_mut()
            .ok_or_else(|| BackendError::Conflict("元数据对象属性无效".into()))?
            .insert("discovery_status".into(), json!("missing"));
        update(conn, params, &id, version, "inactive", &properties).await?;
        sqlx::query("UPDATE metadata_catalog SET discovery_status='missing',object_version=$1,last_collection_id=$2,
            last_observed_at=CURRENT_TIMESTAMP WHERE tenant_id=$3 AND object_id=$4")
            .bind(version).bind(&params.collection_id).bind(&params.tenant_id).bind(&id).execute(&mut *conn).await.map_err(error)?;
        storage::audit(conn,params,"metadata_object",&id,"update",json!({"event":"metadata_missing","before_version":resource.version,"after_version":version,"job_id":params.job_id})).await?;
        result.missing_objects += 1;
    }
    Ok(())
}
