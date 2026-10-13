// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use dts_backend::{
    BackendError,
    repo::{master_data::validation, master_data::*, product_resource::*},
};
use sqlx::Row;

use super::{PgMasterDataRepo, models, storage};

pub(super) async fn save(
    repo: &PgMasterDataRepo,
    params: &SaveMasterResourceParams,
) -> Result<ProductResource, BackendError> {
    let mut tx = repo.begin(&params.tenant_id, &params.actor_id).await?;
    let before = storage::before(&mut tx, "master_data_record", params).await?;
    if before
        .as_ref()
        .is_some_and(|record| !matches!(record.status.as_str(), "draft" | "rejected" | "approved"))
    {
        return Err(BackendError::Conflict("审批中的记录不能编辑".into()));
    }
    let model = validation::text(&params.fields.properties, "model_id", 128)?;
    let key = validation::text(&params.fields.properties, "business_key", 1024)?;
    let (model_version, schema) = models::schema(&mut tx, &params.tenant_id, model).await?;
    let fingerprint = validation::record(&schema, &params.fields.properties)?;
    let identity = sqlx::query("SELECT model_id,business_key,key_fingerprint,approved_version FROM master_record_identity WHERE tenant_id = $1 AND record_id = $2")
        .bind(&params.tenant_id).bind(&params.id).fetch_optional(&mut *tx).await.map_err(storage::error)?;
    if let Some(identity) = &identity {
        if identity
            .try_get::<Option<i64>, _>("approved_version")
            .map_err(storage::error)?
            .is_some()
            && (identity
                .try_get::<String, _>("model_id")
                .map_err(storage::error)?
                != model
                || identity
                    .try_get::<String, _>("business_key")
                    .map_err(storage::error)?
                    != key
                || identity
                    .try_get::<String, _>("key_fingerprint")
                    .map_err(storage::error)?
                    != fingerprint)
        {
            return Err(BackendError::Conflict(
                "批准后的模型和业务身份不能改变，编辑将生成新草稿修订".into(),
            ));
        }
    } else if before
        .as_ref()
        .is_some_and(|record| record.status == "approved")
    {
        return Err(BackendError::ConstraintViolation(
            "记录缺少真实批准版本".into(),
        ));
    }
    let mut fields = params.fields.clone();
    fields.status = "draft".into();
    fields.references = vec![ResourceReference {
        field: "model_id".into(),
        target_kind: "master_data_model".into(),
        target_id: model.into(),
    }];
    let write = SaveMasterResourceParams {
        tenant_id: params.tenant_id.clone(),
        id: params.id.clone(),
        actor_id: params.actor_id.clone(),
        expected_version: params.expected_version,
        fields,
    };
    let saved = storage::save(&mut tx, "master_data_record", &write, before.as_ref()).await?;
    sqlx::query("INSERT INTO master_record_identity (tenant_id,record_id,model_id,model_version,business_key,key_fingerprint)
        VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT(tenant_id,record_id) DO UPDATE SET model_id=excluded.model_id,
        model_version=excluded.model_version,business_key=excluded.business_key,key_fingerprint=excluded.key_fingerprint,submitted_by=NULL")
        .bind(&params.tenant_id).bind(&params.id).bind(model).bind(model_version).bind(key).bind(fingerprint)
        .execute(&mut *tx).await.map_err(storage::error)?;
    revision(&mut tx, &saved, model_version, &params.actor_id, None).await?;
    tx.commit().await.map_err(storage::error)?;
    Ok(saved)
}

pub(super) async fn transition(
    repo: &PgMasterDataRepo,
    params: &MasterRecordActionParams,
) -> Result<ProductResource, BackendError> {
    let scope = &params.scope;
    if params.reason.len() > 4000 || params.reason.contains('\0') {
        return Err(BackendError::ConstraintViolation(
            "意见最多4000字节且不能包含NUL".into(),
        ));
    }
    let mut tx = repo.begin(&scope.tenant_id, &scope.actor_id).await?;
    let before = storage::current(&mut tx, "master_data_record", scope).await?;
    let identity = sqlx::query("SELECT model_id,model_version,submitted_by FROM master_record_identity WHERE tenant_id = $1 AND record_id = $2")
        .bind(&scope.tenant_id).bind(&scope.id).fetch_optional(&mut *tx).await.map_err(storage::error)?
        .ok_or_else(|| BackendError::ConstraintViolation("记录缺少主数据身份".into()))?;
    let model_version: i64 = identity.try_get("model_version").map_err(storage::error)?;
    let model_id: String = identity.try_get("model_id").map_err(storage::error)?;
    let snapshot: String = sqlx::query_scalar("SELECT schema_snapshot::text AS schema_snapshot FROM master_model_schema WHERE tenant_id=$1 AND model_id=$2 AND model_version=$3")
        .bind(&scope.tenant_id).bind(&model_id).bind(model_version).fetch_one(&mut *tx).await.map_err(storage::error)?;
    validation::record(
        &serde_json::from_str(&snapshot).map_err(|e| BackendError::Internal(e.to_string()))?,
        &before.properties,
    )?;
    let status = match params.action {
        MasterRecordAction::Submit => {
            if !matches!(before.status.as_str(), "draft" | "rejected") {
                return Err(BackendError::Conflict("仅草稿或驳回记录可以提交".into()));
            }
            "pending"
        },
        MasterRecordAction::Approve | MasterRecordAction::Reject => {
            if before.status != "pending" {
                return Err(BackendError::Conflict("仅待审批记录可以审核".into()));
            }
            let submitter: Option<String> =
                identity.try_get("submitted_by").map_err(storage::error)?;
            if before.created_by == scope.actor_id || submitter.as_deref() == Some(&scope.actor_id)
            {
                return Err(BackendError::ConstraintViolation(
                    "不能审核本人创建或提交的主数据".into(),
                ));
            }
            validate_approver(&mut tx, scope).await?;
            if params.reason.trim().is_empty() || params.reason.len() > 4000 {
                return Err(BackendError::ConstraintViolation(
                    "审批意见不能为空且最多4000字节".into(),
                ));
            }
            if matches!(params.action, MasterRecordAction::Approve) {
                "approved"
            } else {
                "rejected"
            }
        },
    };
    let saved = storage::save(
        &mut tx,
        "master_data_record",
        &storage::params(scope, storage::changed_fields(&before, status)),
        Some(&before),
    )
    .await?;
    sqlx::query("UPDATE master_record_identity SET submitted_by=CASE WHEN $1='pending' THEN $2 ELSE submitted_by END,
        approved_version=CASE WHEN $1='approved' THEN $3 ELSE approved_version END WHERE tenant_id=$4 AND record_id=$5")
        .bind(status).bind(&scope.actor_id).bind(saved.version).bind(&scope.tenant_id).bind(&scope.id)
        .execute(&mut *tx).await.map_err(storage::error)?;
    revision(
        &mut tx,
        &saved,
        model_version,
        &scope.actor_id,
        if params.reason.trim().is_empty() {
            None
        } else {
            Some(params.reason.trim())
        },
    )
    .await?;
    tx.commit().await.map_err(storage::error)?;
    Ok(saved)
}

async fn validate_approver(
    conn: &mut sqlx::PgConnection,
    scope: &MasterScope,
) -> Result<(), BackendError> {
    let permission:Option<String> = sqlx::query_scalar("SELECT rp.permission FROM role_permission rp JOIN user_role ur ON rp.role_id=ur.role_id
        JOIN role r ON r.id=ur.role_id WHERE ur.user_id=$1 AND r.tenant_id=$2 AND rp.permission='master_data:approve' LIMIT 1 FOR SHARE OF rp,ur,r")
        .bind(&scope.actor_id).bind(&scope.tenant_id).fetch_optional(conn).await.map_err(storage::error)?;
    if permission.is_none() {
        return Err(BackendError::ConstraintViolation(
            "审批者没有 master_data:approve 权限".into(),
        ));
    }
    Ok(())
}

async fn revision(
    conn: &mut sqlx::PgConnection,
    record: &ProductResource,
    model_version: i64,
    actor: &str,
    reason: Option<&str>,
) -> Result<(), BackendError> {
    let values = serde_json::to_string(&record.properties["values"])
        .map_err(|e| BackendError::Internal(e.to_string()))?;
    sqlx::query("INSERT INTO master_record_revision (tenant_id,record_id,record_version,model_id,model_version,business_key,record_values,status,actor_id,reason)
        VALUES ($1,$2,$3,$4,$5,$6,$7::jsonb,$8,$9,$10)")
        .bind(&record.tenant_id).bind(&record.id).bind(record.version).bind(record.properties["model_id"].as_str()).bind(model_version)
        .bind(record.properties["business_key"].as_str()).bind(values).bind(&record.status).bind(actor).bind(reason)
        .execute(conn).await.map_err(storage::error)?;
    Ok(())
}

pub(super) async fn approved(
    repo: &PgMasterDataRepo,
    filter: &MasterApprovedFilter,
) -> Result<MasterApprovedPage, BackendError> {
    let mut tx = repo.pool.begin().await.map_err(storage::error)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(storage::error)?;
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM master_record_identity WHERE tenant_id=$1 AND model_id=$2 AND approved_version IS NOT NULL")
        .bind(&filter.tenant_id).bind(&filter.model_id).fetch_one(&mut *tx).await.map_err(storage::error)?;
    let rows=sqlx::query("SELECT r.record_id,r.record_version,r.model_id,r.model_version,r.business_key,r.record_values::text AS record_values,r.actor_id,r.created_at
        FROM master_record_revision r JOIN master_record_identity i ON i.tenant_id=r.tenant_id AND i.record_id=r.record_id AND i.approved_version=r.record_version
        WHERE r.tenant_id=$1 AND r.model_id=$2 AND r.status='approved' ORDER BY r.record_id LIMIT $3 OFFSET $4")
        .bind(&filter.tenant_id).bind(&filter.model_id).bind(i64::from(filter.limit.clamp(1,100))).bind(i64::from(filter.offset))
        .fetch_all(&mut *tx).await.map_err(storage::error)?;
    let data = rows
        .iter()
        .map(|row| {
            let date: DateTime<Utc> = row.try_get("created_at").map_err(storage::error)?;
            Ok(MasterApprovedRecord {
                record_id: row.try_get("record_id").map_err(storage::error)?,
                record_version: row.try_get("record_version").map_err(storage::error)?,
                model_id: row.try_get("model_id").map_err(storage::error)?,
                model_version: row.try_get("model_version").map_err(storage::error)?,
                business_key: row.try_get("business_key").map_err(storage::error)?,
                values: serde_json::from_str(
                    &row.try_get::<String, _>("record_values")
                        .map_err(storage::error)?,
                )
                .map_err(|e| BackendError::Internal(e.to_string()))?,
                approved_by: row.try_get("actor_id").map_err(storage::error)?,
                approved_at: date,
            })
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    tx.commit().await.map_err(storage::error)?;
    Ok(MasterApprovedPage { data, total })
}
