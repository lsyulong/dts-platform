// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::Utc;
use dts_backend::{BackendError, repo::product_resource::*};
use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool, Transaction};

mod assets;
mod external;
pub(crate) mod helpers;
mod lineage;
pub(crate) use helpers::map_error as map_reference_error;
use helpers::{audit, decode_resource, map_error, page_limit, validate_graph};

pub struct SqliteProductResourceRepo {
    pool: SqlitePool,
}

impl SqliteProductResourceRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn begin_write(&self, tenant_id: &str) -> Result<Transaction<'_, Sqlite>, BackendError> {
        // Reserve the writer before validating references; a deferred SQLite
        // transaction permits another writer to change the graph during checks.
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(map_error)?;
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM tenant WHERE id = ?1")
            .bind(tenant_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?;
        if exists.is_none() {
            return Err(BackendError::NotFound("租户不存在".into()));
        }
        Ok(tx)
    }

    pub(crate) async fn read_resource(
        conn: &mut SqliteConnection,
        tenant_id: &str,
        kind: &str,
        id: &str,
    ) -> Result<Option<ProductResource>, BackendError> {
        let row = sqlx::query(
            "SELECT id, tenant_id, kind, code, name, description, parent_id, status,
                    properties, version, created_by, created_at, updated_at
             FROM product_resource WHERE tenant_id = ?1 AND kind = ?2 AND id = ?3",
        )
        .bind(tenant_id)
        .bind(kind)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut resource = decode_resource(&row)?;
        resource.references = Self::read_references(conn, tenant_id, kind, id).await?;
        Ok(Some(resource))
    }

    async fn read_references(
        conn: &mut SqliteConnection,
        tenant_id: &str,
        kind: &str,
        id: &str,
    ) -> Result<Vec<ResourceReference>, BackendError> {
        let rows = sqlx::query(
            "SELECT field, target_kind, target_id FROM product_resource_links
             WHERE tenant_id = ?1 AND source_kind = ?2 AND source_id = ?3
             UNION ALL
             SELECT field, target_kind, target_id FROM product_resource_external_links
             WHERE tenant_id = ?1 AND source_kind = ?2 AND source_id = ?3
             ORDER BY field, target_kind, target_id",
        )
        .bind(tenant_id)
        .bind(kind)
        .bind(id)
        .fetch_all(conn)
        .await
        .map_err(map_error)?;
        Ok(rows
            .iter()
            .map(|row| ResourceReference {
                field: row.get("field"),
                target_kind: row.get("target_kind"),
                target_id: row.get("target_id"),
            })
            .collect())
    }

    pub(crate) async fn write_references(
        conn: &mut SqliteConnection,
        tenant_id: &str,
        kind: &str,
        id: &str,
        references: &[ResourceReference],
    ) -> Result<(), BackendError> {
        sqlx::query(
            "DELETE FROM product_resource_links
             WHERE tenant_id = ?1 AND source_kind = ?2 AND source_id = ?3",
        )
        .bind(tenant_id)
        .bind(kind)
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(map_error)?;
        sqlx::query(
            "DELETE FROM product_resource_external_links
             WHERE tenant_id = ?1 AND source_kind = ?2 AND source_id = ?3",
        )
        .bind(tenant_id)
        .bind(kind)
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(map_error)?;
        for reference in references {
            if external::is_external(&reference.target_kind) {
                external::write(&mut *conn, tenant_id, kind, id, reference).await?;
                continue;
            }
            sqlx::query(
                "INSERT INTO product_resource_links
                 (tenant_id, source_kind, source_id, field, target_kind, target_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(tenant_id)
            .bind(kind)
            .bind(id)
            .bind(&reference.field)
            .bind(&reference.target_kind)
            .bind(&reference.target_id)
            .execute(&mut *conn)
            .await
            .map_err(map_error)?;
        }
        Ok(())
    }
}

#[async_trait]
impl ProductResourceRepo for SqliteProductResourceRepo {
    async fn list(&self, filter: &ProductResourceFilter) -> Result<ResourcePage, BackendError> {
        let query = filter.q.as_ref().map(|q| {
            format!(
                "%{}%",
                q.to_lowercase()
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        });
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM product_resource
             WHERE tenant_id = ?1 AND kind = ?2 AND (?3 IS NULL OR status = ?3)
               AND (?5 IS NULL OR created_by = ?5)
               AND (?4 IS NULL OR lower(name) LIKE ?4 ESCAPE '\\'
                    OR lower(code) LIKE ?4 ESCAPE '\\')",
        )
        .bind(&filter.tenant_id)
        .bind(&filter.kind)
        .bind(&filter.status)
        .bind(&query)
        .bind(&filter.created_by)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_error)?;
        let rows = sqlx::query(
            "SELECT id, tenant_id, kind, code, name, description, parent_id, status,
                    properties, version, created_by, created_at, updated_at
             FROM product_resource
             WHERE tenant_id = ?1 AND kind = ?2 AND (?3 IS NULL OR status = ?3)
               AND (?7 IS NULL OR created_by = ?7)
               AND (?4 IS NULL OR lower(name) LIKE ?4 ESCAPE '\\'
                    OR lower(code) LIKE ?4 ESCAPE '\\')
             ORDER BY updated_at DESC, id LIMIT ?5 OFFSET ?6",
        )
        .bind(&filter.tenant_id)
        .bind(&filter.kind)
        .bind(&filter.status)
        .bind(&query)
        .bind(page_limit(filter.limit))
        .bind(i64::from(filter.offset))
        .bind(&filter.created_by)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_error)?;
        let mut data = Vec::with_capacity(rows.len());
        for row in rows {
            let mut resource = decode_resource(&row)?;
            resource.references =
                Self::read_references(&mut tx, &filter.tenant_id, &filter.kind, &resource.id)
                    .await?;
            data.push(resource);
        }
        tx.commit().await.map_err(map_error)?;
        Ok(ResourcePage { data, total })
    }

    async fn get(
        &self,
        tenant_id: &str,
        kind: &str,
        id: &str,
    ) -> Result<Option<ProductResource>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let resource = Self::read_resource(&mut tx, tenant_id, kind, id).await?;
        tx.commit().await.map_err(map_error)?;
        Ok(resource)
    }

    async fn create(&self, params: &CreateResourceParams) -> Result<ProductResource, BackendError> {
        lineage::validate_actor(&params.kind, &params.fields, &params.actor_id)?;
        let mut tx = self.begin_write(&params.tenant_id).await?;
        let fields = &params.fields;
        validate_graph(&mut tx, &params.tenant_id, &params.kind, &params.id, fields).await?;
        let properties = serde_json::to_string(&fields.properties)
            .map_err(|e| BackendError::Internal(format!("资源属性序列化失败: {e}")))?;
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO product_resource
             (id, tenant_id, kind, code, name, description, parent_id, status, properties,
              version, created_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11, ?11)",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.kind)
        .bind(&fields.code)
        .bind(&fields.name)
        .bind(&fields.description)
        .bind(&fields.parent_id)
        .bind(&fields.status)
        .bind(&properties)
        .bind(&params.actor_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        Self::write_references(
            &mut tx,
            &params.tenant_id,
            &params.kind,
            &params.id,
            &fields.references,
        )
        .await?;
        let resource = Self::read_resource(&mut tx, &params.tenant_id, &params.kind, &params.id)
            .await?
            .ok_or_else(|| BackendError::Internal("创建资源后未找到记录".into()))?;
        audit(&mut tx, &params.actor_id, "create", None, Some(&resource)).await?;
        tx.commit().await.map_err(map_error)?;
        Ok(resource)
    }

    async fn update(
        &self,
        tenant_id: &str,
        kind: &str,
        id: &str,
        params: &UpdateResourceParams,
    ) -> Result<ProductResource, BackendError> {
        lineage::validate_actor(kind, &params.fields, &params.actor_id)?;
        let mut tx = self.begin_write(tenant_id).await?;
        let before = Self::read_resource(&mut tx, tenant_id, kind, id)
            .await?
            .ok_or_else(|| BackendError::NotFound("资源不存在".into()))?;
        if before.version != params.expected_version {
            return Err(BackendError::Conflict("资源已变更，请刷新后重试".into()));
        }
        let next_version = before
            .version
            .checked_add(1)
            .ok_or_else(|| BackendError::Conflict("资源版本已达到上限".into()))?;
        let fields = &params.fields;
        validate_graph(&mut tx, tenant_id, kind, id, fields).await?;
        let properties = serde_json::to_string(&fields.properties)
            .map_err(|e| BackendError::Internal(format!("资源属性序列化失败: {e}")))?;
        let affected = sqlx::query(
            "UPDATE product_resource
             SET code = ?1, name = ?2, description = ?3, parent_id = ?4, status = ?5,
                 properties = ?6, version = ?7, updated_at = ?8
             WHERE tenant_id = ?9 AND kind = ?10 AND id = ?11 AND version = ?12",
        )
        .bind(&fields.code)
        .bind(&fields.name)
        .bind(&fields.description)
        .bind(&fields.parent_id)
        .bind(&fields.status)
        .bind(&properties)
        .bind(next_version)
        .bind(Utc::now().to_rfc3339())
        .bind(tenant_id)
        .bind(kind)
        .bind(id)
        .bind(params.expected_version)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?
        .rows_affected();
        if affected != 1 {
            return Err(BackendError::Conflict("资源已变更，请刷新后重试".into()));
        }
        Self::write_references(&mut tx, tenant_id, kind, id, &fields.references).await?;
        let resource = Self::read_resource(&mut tx, tenant_id, kind, id)
            .await?
            .ok_or_else(|| BackendError::Internal("更新资源后未找到记录".into()))?;
        audit(
            &mut tx,
            &params.actor_id,
            "update",
            Some(&before),
            Some(&resource),
        )
        .await?;
        tx.commit().await.map_err(map_error)?;
        Ok(resource)
    }

    async fn delete(
        &self,
        tenant_id: &str,
        kind: &str,
        id: &str,
        expected_version: i64,
        actor_id: &str,
    ) -> Result<(), BackendError> {
        let mut tx = self.begin_write(tenant_id).await?;
        let before = Self::read_resource(&mut tx, tenant_id, kind, id)
            .await?
            .ok_or_else(|| BackendError::NotFound("资源不存在".into()))?;
        if before.version != expected_version {
            return Err(BackendError::Conflict("资源已变更，请刷新后重试".into()));
        }
        let children: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM product_resource
             WHERE tenant_id = ?1 AND kind = ?2 AND parent_id = ?3",
        )
        .bind(tenant_id)
        .bind(kind)
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_error)?;
        let references: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM product_resource_links
             WHERE tenant_id = ?1 AND target_kind = ?2 AND target_id = ?3",
        )
        .bind(tenant_id)
        .bind(kind)
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_error)?;
        if children > 0 || references > 0 {
            return Err(BackendError::Conflict(
                "资源仍有子项或被其他资源引用".into(),
            ));
        }
        // Remove outgoing links explicitly; incoming links were checked under
        // the same graph write lock and are also protected by foreign keys.
        Self::write_references(&mut tx, tenant_id, kind, id, &[]).await?;
        let affected = sqlx::query(
            "DELETE FROM product_resource
             WHERE tenant_id = ?1 AND kind = ?2 AND id = ?3 AND version = ?4",
        )
        .bind(tenant_id)
        .bind(kind)
        .bind(id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?
        .rows_affected();
        if affected != 1 {
            return Err(BackendError::Conflict("资源已变更，请刷新后重试".into()));
        }
        audit(&mut tx, actor_id, "delete", Some(&before), None).await?;
        tx.commit().await.map_err(map_error)?;
        Ok(())
    }

    async fn list_audit(
        &self,
        tenant_id: &str,
        kind: Option<&str>,
        resource_id: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<ResourceAuditPage, BackendError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM product_resource_audit WHERE tenant_id = ?1
             AND (?2 IS NULL OR resource_kind = ?2) AND (?3 IS NULL OR resource_id = ?3)",
        )
        .bind(tenant_id)
        .bind(kind)
        .bind(resource_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_error)?;
        let rows = sqlx::query(
            "SELECT id, tenant_id, actor_id, resource_kind, resource_id, operation,
                    summary, created_at FROM product_resource_audit WHERE tenant_id = ?1
             AND (?2 IS NULL OR resource_kind = ?2) AND (?3 IS NULL OR resource_id = ?3)
             ORDER BY created_at DESC, id LIMIT ?4 OFFSET ?5",
        )
        .bind(tenant_id)
        .bind(kind)
        .bind(resource_id)
        .bind(page_limit(limit))
        .bind(i64::from(offset))
        .fetch_all(&mut *tx)
        .await
        .map_err(map_error)?;
        let data = rows
            .iter()
            .map(helpers::decode_audit)
            .collect::<Result<_, _>>()?;
        tx.commit().await.map_err(map_error)?;
        Ok(ResourceAuditPage { data, total })
    }
}
