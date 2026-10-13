// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::product_resource::*};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row, postgres::PgRow};

pub(crate) fn map_error(error: sqlx::Error) -> BackendError {
    if let sqlx::Error::Database(db) = &error {
        if db.is_unique_violation() {
            return BackendError::Conflict("资源编码、标识或引用已存在".into());
        }
        if db.is_foreign_key_violation() {
            return BackendError::Conflict("资源关联不存在或仍被引用".into());
        }
        if db.code().as_deref() == Some("23514") {
            return BackendError::ConstraintViolation("引用对象已失效、被引用或用户已停用".into());
        }
    }
    dts_backend_rdbms::map_sqlx_error(error)
}

pub(super) fn page_limit(limit: u32) -> i64 {
    i64::from(limit.clamp(1, 100))
}

fn decode_json(text: &str) -> Result<Value, BackendError> {
    serde_json::from_str(text)
        .map_err(|e| BackendError::Internal(format!("持久化资源 JSON 无效: {e}")))
}

fn decode_time(row: &PgRow, field: &str) -> Result<DateTime<Utc>, BackendError> {
    row.try_get(field).map_err(map_error)
}

pub(super) fn decode_resource(row: &PgRow) -> Result<ProductResource, BackendError> {
    Ok(ProductResource {
        id: row.try_get("id").map_err(map_error)?,
        tenant_id: row.try_get("tenant_id").map_err(map_error)?,
        kind: row.try_get("kind").map_err(map_error)?,
        code: row.try_get("code").map_err(map_error)?,
        name: row.try_get("name").map_err(map_error)?,
        description: row.try_get("description").map_err(map_error)?,
        parent_id: row.try_get("parent_id").map_err(map_error)?,
        status: row.try_get("status").map_err(map_error)?,
        properties: decode_json(&row.try_get::<String, _>("properties").map_err(map_error)?)?,
        version: row.try_get("version").map_err(map_error)?,
        created_by: row.try_get("created_by").map_err(map_error)?,
        created_at: decode_time(row, "created_at")?,
        updated_at: decode_time(row, "updated_at")?,
        references: Vec::new(),
    })
}

pub(super) fn decode_audit(row: &PgRow) -> Result<ResourceAudit, BackendError> {
    Ok(ResourceAudit {
        id: row.try_get("id").map_err(map_error)?,
        tenant_id: row.try_get("tenant_id").map_err(map_error)?,
        actor_id: row.try_get("actor_id").map_err(map_error)?,
        resource_kind: row.try_get("resource_kind").map_err(map_error)?,
        resource_id: row.try_get("resource_id").map_err(map_error)?,
        operation: row.try_get("operation").map_err(map_error)?,
        summary: decode_json(&row.try_get::<String, _>("summary").map_err(map_error)?)?,
        created_at: decode_time(row, "created_at")?,
    })
}

pub(crate) async fn validate_graph(
    conn: &mut PgConnection,
    tenant_id: &str,
    kind: &str,
    id: &str,
    fields: &ResourceWrite,
) -> Result<(), BackendError> {
    if kind == "metadata_lineage" {
        super::lineage::validate(&mut *conn, tenant_id, id, fields).await?;
    }
    let mut parent = fields.parent_id.clone();
    let mut visited = HashSet::from([id.to_owned()]);
    while let Some(parent_id) = parent {
        if !visited.insert(parent_id.clone()) {
            return Err(BackendError::Conflict("资源父子关系不能形成环".into()));
        }
        let row = sqlx::query(
            "SELECT parent_id FROM product_resource
             WHERE tenant_id = $1 AND kind = $2 AND id = $3",
        )
        .bind(tenant_id)
        .bind(kind)
        .bind(&parent_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_error)?
        .ok_or_else(|| {
            BackendError::ConstraintViolation("父资源不存在或不属于当前租户和类型".into())
        })?;
        parent = row.try_get("parent_id").map_err(map_error)?;
    }
    let mut references = HashSet::new();
    for reference in &fields.references {
        if !references.insert((
            &reference.field,
            &reference.target_kind,
            &reference.target_id,
        )) {
            return Err(BackendError::ConstraintViolation("资源引用重复".into()));
        }
        if super::external::validate(&mut *conn, tenant_id, reference).await? {
            continue;
        }
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT id FROM product_resource WHERE tenant_id = $1 AND kind = $2 AND id = $3",
        )
        .bind(tenant_id)
        .bind(&reference.target_kind)
        .bind(&reference.target_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_error)?;
        if exists.is_none() {
            return Err(BackendError::ConstraintViolation(
                "引用资源不存在或不属于当前租户和类型".into(),
            ));
        }
        super::external::validate_metric_cycle(&mut *conn, tenant_id, kind, id, reference).await?;
    }
    Ok(())
}

fn safe_summary(resource: &ProductResource) -> Value {
    // Audit only identifiers and state. Properties and descriptions may contain
    // credentials or business content and must never be copied into audit logs.
    json!({
        "code": resource.code,
        "name": resource.name,
        "status": resource.status,
        "version": resource.version,
        "parent_id": resource.parent_id,
        "references_count": resource.references.len(),
    })
}

pub(crate) async fn audit(
    conn: &mut PgConnection,
    actor_id: &str,
    operation: &str,
    before: Option<&ProductResource>,
    after: Option<&ProductResource>,
) -> Result<(), BackendError> {
    let resource = after
        .or(before)
        .ok_or_else(|| BackendError::Internal("审计缺少资源".into()))?;
    let summary = json!({
        "before": before.map(safe_summary),
        "after": after.map(safe_summary),
        "action": operation,
    })
    .to_string();
    let operation = match (before, after) {
        (None, Some(_)) => "create",
        (Some(_), None) => "delete",
        _ => "update",
    };
    sqlx::query(
        "INSERT INTO product_resource_audit
         (id, tenant_id, actor_id, resource_kind, resource_id, operation, summary, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8::timestamptz)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&resource.tenant_id)
    .bind(actor_id)
    .bind(&resource.kind)
    .bind(&resource.id)
    .bind(operation)
    .bind(summary)
    .bind(Utc::now().to_rfc3339())
    .execute(conn)
    .await
    .map_err(map_error)?;
    Ok(())
}
