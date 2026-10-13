// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{BackendError, repo::product_resource::ResourceReference};
use sqlx::PgConnection;

use super::helpers::map_error;

pub(super) fn is_external(kind: &str) -> bool {
    matches!(kind, "user" | "role" | "datasource")
}

pub(super) async fn validate(
    conn: &mut PgConnection,
    tenant_id: &str,
    reference: &ResourceReference,
) -> Result<bool, BackendError> {
    let sql = match reference.target_kind.as_str() {
        "user" => {
            "SELECT id FROM \"user\" WHERE tenant_id = $1 AND id = $2 AND status = 'active' FOR UPDATE"
        },
        "role" => "SELECT id FROM role WHERE tenant_id = $1 AND id = $2 FOR KEY SHARE",
        "datasource" => "SELECT id FROM datasource WHERE tenant_id = $1 AND id = $2 FOR KEY SHARE",
        _ => return Ok(false),
    };
    let found: Option<String> = sqlx::query_scalar(sql)
        .bind(tenant_id)
        .bind(&reference.target_id)
        .fetch_optional(conn)
        .await
        .map_err(map_error)?;
    if found.is_none() {
        return Err(BackendError::ConstraintViolation(
            "引用身份或数据源不存在、不属于当前租户或用户已停用".into(),
        ));
    }
    Ok(true)
}

pub(super) async fn write(
    conn: &mut PgConnection,
    tenant_id: &str,
    kind: &str,
    id: &str,
    reference: &ResourceReference,
) -> Result<(), BackendError> {
    let (user, role, datasource) = match reference.target_kind.as_str() {
        "user" => (Some(&reference.target_id), None, None),
        "role" => (None, Some(&reference.target_id), None),
        "datasource" => (None, None, Some(&reference.target_id)),
        _ => return Err(BackendError::ConstraintViolation("外部引用类型无效".into())),
    };
    sqlx::query(
        "INSERT INTO product_resource_external_links
         (tenant_id, source_kind, source_id, field, target_kind, target_id,
          user_id, user_status, role_id, datasource_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(tenant_id)
    .bind(kind)
    .bind(id)
    .bind(&reference.field)
    .bind(&reference.target_kind)
    .bind(&reference.target_id)
    .bind(user)
    .bind(user.map(|_| "active"))
    .bind(role)
    .bind(datasource)
    .execute(conn)
    .await
    .map_err(map_error)?;
    Ok(())
}

pub(super) async fn validate_metric_cycle(
    conn: &mut PgConnection,
    tenant_id: &str,
    kind: &str,
    id: &str,
    reference: &ResourceReference,
) -> Result<(), BackendError> {
    if kind != "composite_metric" || reference.field != "metric_ids" {
        return Ok(());
    }
    if reference.target_id == id {
        return Err(BackendError::ConstraintViolation(
            "复合指标不能引用自身".into(),
        ));
    }
    let cyclic: i64 = sqlx::query_scalar(
        "WITH RECURSIVE dependencies(id) AS (
            SELECT target_id FROM product_resource_links
            WHERE tenant_id = $1 AND source_kind = 'composite_metric'
              AND source_id = $2 AND field = 'metric_ids'
            UNION
            SELECT l.target_id FROM product_resource_links l JOIN dependencies d ON l.source_id = d.id
            WHERE l.tenant_id = $1 AND l.source_kind = 'composite_metric' AND l.field = 'metric_ids'
         ) SELECT COUNT(*) FROM dependencies WHERE id = $3",
    )
    .bind(tenant_id)
    .bind(&reference.target_id)
    .bind(id)
    .fetch_one(conn)
    .await
    .map_err(map_error)?;
    if cyclic != 0 {
        return Err(BackendError::ConstraintViolation(
            "复合指标依赖不能形成环".into(),
        ));
    }
    Ok(())
}
