// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{
    BackendError,
    repo::{master_data::validation, master_data::*, product_resource::ProductResource},
};
use serde_json::Value;

use super::{SqliteMasterDataRepo, storage};

pub(super) async fn save(
    repo: &SqliteMasterDataRepo,
    params: &SaveMasterResourceParams,
) -> Result<ProductResource, BackendError> {
    validation::schema(&params.fields.properties)?;
    let mut tx = repo.begin(&params.tenant_id, &params.actor_id).await?;
    let before = storage::before(&mut tx, "master_data_model", params).await?;
    if before
        .as_ref()
        .is_some_and(|record| record.status != "draft")
    {
        return Err(BackendError::Conflict(
            "已冻结的主数据模型不能原地修改，请新建模型版本".into(),
        ));
    }
    let mut fields = params.fields.clone();
    fields.status = "draft".into();
    fields.references = match fields.properties.get("owner_id") {
        Some(Value::String(owner)) if !owner.is_empty() => {
            vec![dts_backend::repo::product_resource::ResourceReference {
                field: "owner_id".into(),
                target_kind: "user".into(),
                target_id: owner.clone(),
            }]
        },
        None | Some(Value::String(_)) => vec![],
        _ => {
            return Err(BackendError::ConstraintViolation(
                "负责人必须是实际用户ID".into(),
            ));
        },
    };
    let write = SaveMasterResourceParams {
        tenant_id: params.tenant_id.clone(),
        id: params.id.clone(),
        actor_id: params.actor_id.clone(),
        expected_version: params.expected_version,
        fields,
    };
    let saved = storage::save(&mut tx, "master_data_model", &write, before.as_ref()).await?;
    tx.commit().await.map_err(storage::error)?;
    Ok(saved)
}

pub(super) async fn publish(
    repo: &SqliteMasterDataRepo,
    scope: &MasterScope,
) -> Result<MasterModelPublication, BackendError> {
    let mut tx = repo.begin(&scope.tenant_id, &scope.actor_id).await?;
    let before = storage::current(&mut tx, "master_data_model", scope).await?;
    validation::schema(&before.properties)?;
    if before.status != "draft" {
        return Err(BackendError::Conflict("仅草稿模型可以冻结发布".into()));
    }
    let saved = storage::save(
        &mut tx,
        "master_data_model",
        &storage::params(scope, storage::changed_fields(&before, "published")),
        Some(&before),
    )
    .await?;
    let snapshot = serde_json::to_string(&saved.properties)
        .map_err(|e| BackendError::Internal(e.to_string()))?;
    sqlx::query("INSERT INTO master_model_schema (tenant_id, model_id, model_version, schema_snapshot, published_by) VALUES (?1,?2,?3,?4,?5)")
        .bind(&scope.tenant_id).bind(&scope.id).bind(saved.version).bind(snapshot).bind(&scope.actor_id)
        .execute(&mut *tx).await.map_err(storage::error)?;
    tx.commit().await.map_err(storage::error)?;
    Ok(MasterModelPublication {
        model_version: saved.version,
        schema_snapshot: saved.properties.clone(),
        resource: saved,
        physical_published: false,
    })
}

pub(super) async fn schema(
    conn: &mut sqlx::SqliteConnection,
    tenant: &str,
    model: &str,
) -> Result<(i64, Value), BackendError> {
    let source = storage::get(&mut *conn, tenant, "master_data_model", model).await?;
    if source.status != "published" {
        return Err(BackendError::Conflict("主数据模型尚未冻结发布".into()));
    }
    let snapshot: Option<String> = sqlx::query_scalar("SELECT schema_snapshot FROM master_model_schema WHERE tenant_id = ?1 AND model_id = ?2 AND model_version = ?3")
        .bind(tenant).bind(model).bind(source.version).fetch_optional(conn).await.map_err(storage::error)?;
    let snapshot = snapshot
        .ok_or_else(|| BackendError::ConstraintViolation("主数据模型缺少真实冻结版本".into()))?;
    Ok((
        source.version,
        serde_json::from_str(&snapshot).map_err(|e| BackendError::Internal(e.to_string()))?,
    ))
}

pub(super) async fn delete(
    repo: &SqliteMasterDataRepo,
    params: &DeleteMasterResourceParams,
) -> Result<(), BackendError> {
    if !matches!(
        params.kind.as_str(),
        "master_data_model" | "master_data_record" | "master_code_rule"
    ) {
        return Err(BackendError::ConstraintViolation("非主数据类型".into()));
    }
    let scope = &params.scope;
    let mut tx = repo.begin(&scope.tenant_id, &scope.actor_id).await?;
    let before = storage::current(&mut tx, &params.kind, scope).await?;
    if !matches!(before.status.as_str(), "draft" | "rejected") {
        return Err(BackendError::Conflict("仅未提交草稿可以删除".into()));
    }
    let evidence: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM master_record_revision WHERE tenant_id = ?1 AND record_id = ?2 AND status <> 'draft'")
        .bind(&scope.tenant_id).bind(&scope.id).fetch_one(&mut *tx).await.map_err(storage::error)?;
    if evidence != 0 {
        return Err(BackendError::Conflict(
            "记录存在提交或审批历史，不能硬删除".into(),
        ));
    }
    super::super::product_resource::SqliteProductResourceRepo::write_references(
        &mut tx,
        &scope.tenant_id,
        &params.kind,
        &scope.id,
        &[],
    )
    .await?;
    sqlx::query("DELETE FROM product_resource WHERE tenant_id = ?1 AND kind = ?2 AND id = ?3 AND version = ?4")
        .bind(&scope.tenant_id).bind(&params.kind).bind(&scope.id).bind(scope.expected_version)
        .execute(&mut *tx).await.map_err(storage::error)?;
    super::super::product_resource::helpers::audit(
        &mut tx,
        &scope.actor_id,
        "delete",
        Some(&before),
        None,
    )
    .await?;
    tx.commit().await.map_err(storage::error)?;
    Ok(())
}
