// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use chrono::Utc;
use dts_backend::{
    BackendError,
    repo::{master_data::*, product_resource::*},
};
use sqlx::PgConnection;

use super::super::product_resource::{PgProductResourceRepo as Resources, helpers};

pub(super) use helpers::map_error as error;

pub(super) async fn get(
    conn: &mut PgConnection,
    tenant: &str,
    kind: &str,
    id: &str,
) -> Result<ProductResource, BackendError> {
    Resources::read_resource(conn, tenant, kind, id)
        .await?
        .ok_or_else(|| BackendError::NotFound("主数据对象不存在".into()))
}

pub(super) async fn before(
    conn: &mut PgConnection,
    kind: &str,
    params: &SaveMasterResourceParams,
) -> Result<Option<ProductResource>, BackendError> {
    let before = Resources::read_resource(conn, &params.tenant_id, kind, &params.id).await?;
    match (&before, params.expected_version) {
        (Some(resource), Some(version)) if resource.version == version => Ok(before),
        (None, None) => Ok(None),
        _ => Err(BackendError::Conflict(
            "主数据对象不存在或版本已经变更".into(),
        )),
    }
}

pub(super) async fn current(
    conn: &mut PgConnection,
    kind: &str,
    scope: &MasterScope,
) -> Result<ProductResource, BackendError> {
    let resource = get(conn, &scope.tenant_id, kind, &scope.id).await?;
    if resource.version != scope.expected_version {
        return Err(BackendError::Conflict("主数据对象版本已经变更".into()));
    }
    Ok(resource)
}

pub(super) async fn save(
    conn: &mut PgConnection,
    kind: &str,
    params: &SaveMasterResourceParams,
    previous: Option<&ProductResource>,
) -> Result<ProductResource, BackendError> {
    let fields = &params.fields;
    if fields.parent_id.is_some()
        || fields.code.is_empty()
        || fields.code.len() > 128
        || (!fields.code.as_bytes()[0].is_ascii_alphabetic() && !fields.code.starts_with('_'))
        || !fields
            .code
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        || fields.name.trim().is_empty()
        || fields.name.chars().count() > 128
        || fields.name.chars().any(char::is_control)
        || fields
            .description
            .as_ref()
            .is_some_and(|value| value.len() > 8192)
    {
        return Err(BackendError::ConstraintViolation(
            "主数据编码、名称无效或不支持父级".into(),
        ));
    }
    helpers::validate_graph(&mut *conn, &params.tenant_id, kind, &params.id, fields).await?;
    let version = previous.map_or(Ok(1), |resource| {
        resource
            .version
            .checked_add(1)
            .ok_or_else(|| BackendError::Conflict("主数据版本已达上限".into()))
    })?;
    let properties = serde_json::to_string(&fields.properties)
        .map_err(|e| BackendError::Internal(e.to_string()))?;
    if properties.len() > 256 * 1024 {
        return Err(BackendError::ConstraintViolation(
            "主数据属性超过256KiB".into(),
        ));
    }
    let now = Utc::now().to_rfc3339();
    if let Some(previous) = previous {
        let affected = sqlx::query("UPDATE product_resource SET code = $1, name = $2, description = $3, properties = $4::jsonb,
            status = $5, version = $6, updated_at = $7::timestamptz WHERE tenant_id = $8 AND kind = $9 AND id = $10 AND version = $11")
            .bind(&fields.code).bind(&fields.name).bind(&fields.description).bind(properties).bind(&fields.status)
            .bind(version).bind(&now).bind(&params.tenant_id).bind(kind).bind(&params.id).bind(previous.version)
            .execute(&mut *conn).await.map_err(error)?.rows_affected();
        if affected != 1 {
            return Err(BackendError::Conflict("主数据对象已经变更".into()));
        }
    } else {
        sqlx::query("INSERT INTO product_resource (id, tenant_id, kind, code, name, description, status, properties,
            version, created_by, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8::jsonb,1,$9,$10::timestamptz,$10::timestamptz)")
            .bind(&params.id).bind(&params.tenant_id).bind(kind).bind(&fields.code).bind(&fields.name)
            .bind(&fields.description).bind(&fields.status).bind(properties).bind(&params.actor_id).bind(&now)
            .execute(&mut *conn).await.map_err(error)?;
    }
    Resources::write_references(
        &mut *conn,
        &params.tenant_id,
        kind,
        &params.id,
        &fields.references,
    )
    .await?;
    let saved = get(&mut *conn, &params.tenant_id, kind, &params.id).await?;
    helpers::audit(
        conn,
        &params.actor_id,
        if previous.is_some() {
            "update"
        } else {
            "create"
        },
        previous,
        Some(&saved),
    )
    .await?;
    Ok(saved)
}

pub(super) fn changed_fields(resource: &ProductResource, status: &str) -> ResourceWrite {
    ResourceWrite {
        code: resource.code.clone(),
        name: resource.name.clone(),
        description: resource.description.clone(),
        parent_id: None,
        status: status.into(),
        properties: resource.properties.clone(),
        references: resource.references.clone(),
    }
}

pub(super) fn params(scope: &MasterScope, fields: ResourceWrite) -> SaveMasterResourceParams {
    SaveMasterResourceParams {
        tenant_id: scope.tenant_id.clone(),
        id: scope.id.clone(),
        actor_id: scope.actor_id.clone(),
        expected_version: Some(scope.expected_version),
        fields,
    }
}
