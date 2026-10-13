// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::tag_assignment::*};
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::product_resource::map_reference_error as error;

pub struct SqliteTagAssignmentRepo {
    pool: SqlitePool,
}

impl SqliteTagAssignmentRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn timestamp(value: String) -> Result<DateTime<Utc>, BackendError> {
        DateTime::parse_from_rfc3339(&value)
            .map(|v| v.with_timezone(&Utc))
            .map_err(|e| BackendError::Internal(format!("标签绑定时间无效: {e}")))
    }

    fn assignment(row: &sqlx::sqlite::SqliteRow) -> Result<TagAssignment, BackendError> {
        Ok(TagAssignment {
            id: row.try_get("id").map_err(error)?,
            tenant_id: row.try_get("tenant_id").map_err(error)?,
            tag_id: row.try_get("tag_id").map_err(error)?,
            tag_version: row.try_get("tag_version").map_err(error)?,
            target_kind: row.try_get("target_kind").map_err(error)?,
            target_id: row.try_get("target_id").map_err(error)?,
            target_code: row.try_get("target_code").map_err(error)?,
            target_name: row.try_get("target_name").map_err(error)?,
            assigned_by: row.try_get("assigned_by").map_err(error)?,
            assigned_at: Self::timestamp(row.try_get("assigned_at").map_err(error)?)?,
        })
    }

    async fn replay(
        conn: &mut SqliteConnection,
        command: &TagAssignmentCommand,
        action: &str,
        hash: &str,
    ) -> Result<Option<TagAssignmentMutation>, BackendError> {
        let row=sqlx::query("SELECT assignment_id AS id,tenant_id,tag_id,tag_version,target_kind,target_id,target_code,target_name,assigned_by,assigned_at,active,action,request_hash FROM tag_assignment_receipt WHERE tenant_id=?1 AND idempotency_key=?2")
            .bind(&command.tenant_id).bind(&command.idempotency_key).fetch_optional(conn).await.map_err(error)?;
        let Some(row) = row else { return Ok(None) };
        let saved_action: String = row.try_get("action").map_err(error)?;
        let saved_hash: String = row.try_get("request_hash").map_err(error)?;
        if saved_action != action || saved_hash != hash {
            return Err(BackendError::Conflict(
                "幂等键已用于另一标签绑定请求".into(),
            ));
        }
        Ok(Some(TagAssignmentMutation {
            assignment: Self::assignment(&row)?,
            active: row.try_get::<i64, _>("active").map_err(error)? == 1,
            replayed: true,
        }))
    }

    async fn context(
        conn: &mut SqliteConnection,
        command: &TagAssignmentCommand,
    ) -> Result<(String, String), BackendError> {
        let version: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM product_resource WHERE tenant_id=?1 AND kind='tag' AND id=?2",
        )
        .bind(&command.tenant_id)
        .bind(&command.tag_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(error)?;
        let version =
            version.ok_or_else(|| BackendError::NotFound("标签不存在或不属于当前租户".into()))?;
        if version != command.expected_tag_version {
            return Err(BackendError::Conflict(
                "标签定义已更新，请刷新后重试".into(),
            ));
        }
        let target = sqlx::query(
            "SELECT code,name FROM product_resource WHERE tenant_id=?1 AND kind=?2 AND id=?3",
        )
        .bind(&command.tenant_id)
        .bind(&command.target_kind)
        .bind(&command.target_id)
        .fetch_optional(conn)
        .await
        .map_err(error)?
        .ok_or_else(|| BackendError::NotFound("绑定目标不存在或不属于当前租户".into()))?;
        Ok((
            target.try_get("code").map_err(error)?,
            target.try_get("name").map_err(error)?,
        ))
    }

    async fn active(
        conn: &mut SqliteConnection,
        command: &TagAssignmentCommand,
    ) -> Result<Option<TagAssignment>, BackendError> {
        let row=sqlx::query("SELECT a.id,a.tenant_id,a.tag_id,a.tag_version,a.target_kind,a.target_id,r.code AS target_code,r.name AS target_name,a.assigned_by,a.assigned_at FROM tag_assignment a JOIN product_resource r ON r.tenant_id=a.tenant_id AND r.kind=a.target_kind AND r.id=a.target_id WHERE a.tenant_id=?1 AND a.tag_id=?2 AND a.target_kind=?3 AND a.target_id=?4")
            .bind(&command.tenant_id).bind(&command.tag_id).bind(&command.target_kind).bind(&command.target_id).fetch_optional(conn).await.map_err(error)?;
        row.as_ref().map(Self::assignment).transpose()
    }

    async fn receipt(
        conn: &mut SqliteConnection,
        command: &TagAssignmentCommand,
        action: &str,
        hash: &str,
        assignment: &TagAssignment,
        active: bool,
    ) -> Result<(), BackendError> {
        sqlx::query("INSERT INTO tag_assignment_receipt(id,tenant_id,actor_id,idempotency_key,request_hash,action,assignment_id,tag_id,tag_version,target_kind,target_id,target_code,target_name,assigned_by,assigned_at,active,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&command.tenant_id).bind(&command.actor_id).bind(&command.idempotency_key).bind(hash).bind(action)
            .bind(&assignment.id).bind(&assignment.tag_id).bind(assignment.tag_version).bind(&assignment.target_kind).bind(&assignment.target_id)
            .bind(&assignment.target_code).bind(&assignment.target_name).bind(&assignment.assigned_by).bind(assignment.assigned_at.to_rfc3339()).bind(i64::from(active)).bind(Utc::now().to_rfc3339())
            .execute(&mut *conn).await.map_err(error)?;
        let summary=serde_json::json!({"action":action,"target_kind":assignment.target_kind,"target_id":assignment.target_id,"active":active}).to_string();
        sqlx::query("INSERT INTO product_resource_audit(id,tenant_id,actor_id,resource_kind,resource_id,operation,summary,created_at) VALUES(?1,?2,?3,'tag',?4,'update',?5,?6)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&command.tenant_id).bind(&command.actor_id).bind(&command.tag_id).bind(summary).bind(Utc::now().to_rfc3339())
            .execute(conn).await.map_err(error)?;
        Ok(())
    }
}

#[async_trait]
impl TagAssignmentRepo for SqliteTagAssignmentRepo {
    async fn assign(
        &self,
        command: &TagAssignmentCommand,
    ) -> Result<TagAssignmentMutation, BackendError> {
        let hash = validate_command(command, "assign")?;
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(error)?;
        if let Some(result) = Self::replay(&mut tx, command, "assign", &hash).await? {
            tx.commit().await.map_err(error)?;
            return Ok(result);
        }
        let (target_code, target_name) = Self::context(&mut tx, command).await?;
        let assignment = if let Some(existing) = Self::active(&mut tx, command).await? {
            existing
        } else {
            let assignment = TagAssignment {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: command.tenant_id.clone(),
                tag_id: command.tag_id.clone(),
                tag_version: command.expected_tag_version,
                target_kind: command.target_kind.clone(),
                target_id: command.target_id.clone(),
                target_code,
                target_name,
                assigned_by: command.actor_id.clone(),
                assigned_at: Utc::now(),
            };
            sqlx::query("INSERT INTO tag_assignment(id,tenant_id,tag_id,tag_version,target_kind,target_id,assigned_by,assigned_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")
                .bind(&assignment.id).bind(&assignment.tenant_id).bind(&assignment.tag_id).bind(assignment.tag_version).bind(&assignment.target_kind).bind(&assignment.target_id).bind(&assignment.assigned_by).bind(assignment.assigned_at.to_rfc3339()).execute(&mut *tx).await.map_err(error)?;
            assignment
        };
        Self::receipt(&mut tx, command, "assign", &hash, &assignment, true).await?;
        tx.commit().await.map_err(error)?;
        Ok(TagAssignmentMutation {
            assignment,
            active: true,
            replayed: false,
        })
    }

    async fn unassign(
        &self,
        command: &TagAssignmentCommand,
    ) -> Result<TagAssignmentMutation, BackendError> {
        let hash = validate_command(command, "unassign")?;
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(error)?;
        if let Some(result) = Self::replay(&mut tx, command, "unassign", &hash).await? {
            tx.commit().await.map_err(error)?;
            return Ok(result);
        }
        Self::context(&mut tx, command).await?;
        let assignment = Self::active(&mut tx, command)
            .await?
            .ok_or_else(|| BackendError::NotFound("该标签当前没有绑定到目标对象".into()))?;
        sqlx::query("DELETE FROM tag_assignment WHERE tenant_id=?1 AND id=?2")
            .bind(&command.tenant_id)
            .bind(&assignment.id)
            .execute(&mut *tx)
            .await
            .map_err(error)?;
        Self::receipt(&mut tx, command, "unassign", &hash, &assignment, false).await?;
        tx.commit().await.map_err(error)?;
        Ok(TagAssignmentMutation {
            assignment,
            active: false,
            replayed: false,
        })
    }

    async fn list(&self, filter: &TagAssignmentFilter) -> Result<TagAssignmentPage, BackendError> {
        if filter.limit == 0 || filter.limit > 100 {
            return Err(BackendError::ConstraintViolation(
                "标签绑定分页必须为 1 至 100".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(error)?;
        let total = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tag_assignment WHERE tenant_id=?1 AND tag_id=?2",
        )
        .bind(&filter.tenant_id)
        .bind(&filter.tag_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(error)?;
        let rows=sqlx::query("SELECT a.id,a.tenant_id,a.tag_id,a.tag_version,a.target_kind,a.target_id,r.code AS target_code,r.name AS target_name,a.assigned_by,a.assigned_at FROM tag_assignment a JOIN product_resource r ON r.tenant_id=a.tenant_id AND r.kind=a.target_kind AND r.id=a.target_id WHERE a.tenant_id=?1 AND a.tag_id=?2 ORDER BY a.assigned_at DESC,a.id LIMIT ?3 OFFSET ?4")
            .bind(&filter.tenant_id).bind(&filter.tag_id).bind(i64::from(filter.limit)).bind(i64::from(filter.offset)).fetch_all(&mut *tx).await.map_err(error)?;
        let data = rows
            .iter()
            .map(Self::assignment)
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await.map_err(error)?;
        Ok(TagAssignmentPage { data, total })
    }
}
