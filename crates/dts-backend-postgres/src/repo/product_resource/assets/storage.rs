use chrono::Utc;
use dts_backend::{BackendError, ProductResource, repo::assets::*};
use sqlx::PgConnection;

use super::super::{PgProductResourceRepo as Resources, helpers};

pub(super) async fn get(
    conn: &mut PgConnection,
    tenant: &str,
    kind: &str,
    id: &str,
) -> Result<ProductResource, BackendError> {
    Resources::read_resource(conn, tenant, kind, id)
        .await?
        .ok_or_else(|| BackendError::NotFound("资产对象不存在".into()))
}

pub(super) async fn current(
    conn: &mut PgConnection,
    kind: &str,
    scope: &AssetScope,
) -> Result<ProductResource, BackendError> {
    let resource = get(conn, &scope.tenant_id, kind, &scope.id).await?;
    if resource.version != scope.expected_version {
        return Err(BackendError::Conflict("资产版本已经变更".into()));
    }
    Ok(resource)
}

pub(super) async fn write(
    conn: &mut PgConnection,
    params: &SaveAssetParams,
    previous: Option<&ProductResource>,
    operation: &str,
) -> Result<ProductResource, BackendError> {
    let fields = &params.fields;
    helpers::validate_graph(conn, &params.tenant_id, &params.kind, &params.id, fields).await?;
    let properties = serde_json::to_string(&fields.properties)
        .map_err(|e| BackendError::Internal(e.to_string()))?;
    let version = previous.map_or(Ok(1), |v| {
        v.version
            .checked_add(1)
            .ok_or_else(|| BackendError::Conflict("资产版本达到上限".into()))
    })?;
    let now = Utc::now();
    if let Some(before) = previous {
        let affected = sqlx::query("UPDATE product_resource SET code=$1,name=$2,description=$3,status=$4,properties=$5::jsonb,version=$6,updated_at=$7
            WHERE tenant_id=$8 AND kind=$9 AND id=$10 AND version=$11")
            .bind(&fields.code).bind(&fields.name).bind(&fields.description).bind(&fields.status).bind(properties).bind(version).bind(now)
            .bind(&params.tenant_id).bind(&params.kind).bind(&params.id).bind(before.version).execute(&mut *conn).await.map_err(helpers::map_error)?.rows_affected();
        if affected != 1 {
            return Err(BackendError::Conflict("资产版本已经变更".into()));
        }
    } else {
        sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,description,status,properties,version,created_by,created_at,updated_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8::jsonb,1,$9,$10,$10)")
            .bind(&params.id).bind(&params.tenant_id).bind(&params.kind).bind(&fields.code).bind(&fields.name).bind(&fields.description)
            .bind(&fields.status).bind(properties).bind(&params.actor_id).bind(now).execute(&mut *conn).await.map_err(helpers::map_error)?;
    }
    Resources::write_references(
        conn,
        &params.tenant_id,
        &params.kind,
        &params.id,
        &fields.references,
    )
    .await?;
    let saved = get(conn, &params.tenant_id, &params.kind, &params.id).await?;
    helpers::audit(conn, &params.actor_id, operation, previous, Some(&saved)).await?;
    Ok(saved)
}

pub(super) fn transition_params(
    scope: &AssetScope,
    before: &ProductResource,
    status: &str,
) -> SaveAssetParams {
    SaveAssetParams {
        tenant_id: scope.tenant_id.clone(),
        actor_id: scope.actor_id.clone(),
        id: scope.id.clone(),
        kind: "asset_entry".into(),
        expected_version: Some(scope.expected_version),
        fields: dts_backend::ResourceWrite {
            code: before.code.clone(),
            name: before.name.clone(),
            description: before.description.clone(),
            parent_id: None,
            status: status.into(),
            properties: before.properties.clone(),
            references: before.references.clone(),
        },
    }
}
