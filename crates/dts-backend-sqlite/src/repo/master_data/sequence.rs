// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use dts_backend::{
    BackendError,
    repo::{master_data::validation, master_data::*, product_resource::*},
};
use sqlx::Row;

use super::{SqliteMasterDataRepo, models, storage};

pub(super) async fn save(
    repo: &SqliteMasterDataRepo,
    params: &SaveMasterResourceParams,
) -> Result<ProductResource, BackendError> {
    let (prefix, width, start, _) = validation::rule(&params.fields.properties)?;
    let model = validation::text(&params.fields.properties, "model_id", 128)?;
    let mut tx = repo.begin(&params.tenant_id, &params.actor_id).await?;
    models::schema(&mut tx, &params.tenant_id, model).await?;
    let before = storage::before(&mut tx, "master_code_rule", params).await?;
    let sequence=sqlx::query("SELECT model_id,prefix,width,start_value,allocated_count FROM master_code_sequence WHERE tenant_id=?1 AND rule_id=?2")
        .bind(&params.tenant_id).bind(&params.id).fetch_optional(&mut *tx).await.map_err(storage::error)?;
    let allocated = sequence
        .as_ref()
        .map(|row| row.try_get::<i64, _>("allocated_count"))
        .transpose()
        .map_err(storage::error)?
        .unwrap_or(0);
    if let Some(row) = &sequence {
        if allocated > 0
            && (row
                .try_get::<String, _>("model_id")
                .map_err(storage::error)?
                != model
                || row.try_get::<String, _>("prefix").map_err(storage::error)? != prefix
                || row.try_get::<i64, _>("width").map_err(storage::error)? != i64::from(width)
                || row
                    .try_get::<i64, _>("start_value")
                    .map_err(storage::error)?
                    != start)
        {
            return Err(BackendError::Conflict(
                "已经分配编码的规则不能改变模型、前缀、位宽或起始号".into(),
            ));
        }
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
    let saved = storage::save(&mut tx, "master_code_rule", &write, before.as_ref()).await?;
    sqlx::query("INSERT INTO master_code_sequence (tenant_id,rule_id,model_id,prefix,width,start_value,next_value)
        VALUES (?1,?2,?3,?4,?5,?6,?6) ON CONFLICT(tenant_id,rule_id) DO UPDATE SET model_id=excluded.model_id,prefix=excluded.prefix,
        width=excluded.width,start_value=excluded.start_value,next_value=CASE WHEN master_code_sequence.allocated_count=0 THEN excluded.next_value ELSE master_code_sequence.next_value END,
        exhausted=CASE WHEN master_code_sequence.allocated_count=0 THEN 0 ELSE master_code_sequence.exhausted END")
        .bind(&params.tenant_id).bind(&params.id).bind(model).bind(prefix).bind(i64::from(width)).bind(start)
        .execute(&mut *tx).await.map_err(storage::error)?;
    tx.commit().await.map_err(storage::error)?;
    Ok(saved)
}

pub(super) async fn allocate(
    repo: &SqliteMasterDataRepo,
    params: &MasterAllocateParams,
) -> Result<MasterCodeAllocation, BackendError> {
    if params.idempotency_key.trim().is_empty() || params.idempotency_key.len() > 128 {
        return Err(BackendError::ConstraintViolation(
            "编码分配必须有最多128字节的幂等键".into(),
        ));
    }
    let scope = &params.scope;
    let mut tx = repo.begin(&scope.tenant_id, &scope.actor_id).await?;
    let existing=sqlx::query("SELECT id,tenant_id,rule_id,rule_version,model_id,actor_id,idempotency_key,sequence,code,created_at FROM master_code_allocation WHERE tenant_id=?1 AND idempotency_key=?2")
        .bind(&scope.tenant_id).bind(&params.idempotency_key).fetch_optional(&mut *tx).await.map_err(storage::error)?;
    if let Some(existing) = existing {
        let allocation = decode(&existing)?;
        if allocation.rule_id != scope.id
            || allocation.actor_id != scope.actor_id
            || allocation.rule_version != scope.expected_version
        {
            return Err(BackendError::Conflict("幂等键已用于其他分配请求".into()));
        }
        tx.commit().await.map_err(storage::error)?;
        return Ok(allocation);
    }
    let rule = storage::current(&mut tx, "master_code_rule", scope).await?;
    let (prefix, width, _, maximum) = validation::rule(&rule.properties)?;
    let model = validation::text(&rule.properties, "model_id", 128)?;
    models::schema(&mut tx, &scope.tenant_id, model).await?;
    let sequence=sqlx::query("SELECT next_value,exhausted,model_id,prefix,width FROM master_code_sequence WHERE tenant_id=?1 AND rule_id=?2")
        .bind(&scope.tenant_id).bind(&scope.id).fetch_optional(&mut *tx).await.map_err(storage::error)?
        .ok_or_else(||BackendError::ConstraintViolation("编码规则未建立真实序号状态".into()))?;
    let next: i64 = sequence.try_get("next_value").map_err(storage::error)?;
    if sequence
        .try_get::<bool, _>("exhausted")
        .map_err(storage::error)?
        || next > maximum
    {
        return Err(BackendError::Conflict(
            "编码位宽已耗尽，请创建新的编码规则".into(),
        ));
    }
    if sequence
        .try_get::<String, _>("model_id")
        .map_err(storage::error)?
        != model
        || sequence
            .try_get::<String, _>("prefix")
            .map_err(storage::error)?
            != prefix
        || sequence
            .try_get::<i64, _>("width")
            .map_err(storage::error)?
            != i64::from(width)
    {
        return Err(BackendError::Conflict("规则与真实序号状态不一致".into()));
    }
    let code = format!("{prefix}{next:0width$}", width = width as usize);
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO master_code_allocation (id,tenant_id,rule_id,rule_version,model_id,actor_id,idempotency_key,sequence,code) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)")
        .bind(&id).bind(&scope.tenant_id).bind(&scope.id).bind(scope.expected_version).bind(model).bind(&scope.actor_id).bind(&params.idempotency_key).bind(next).bind(code)
        .execute(&mut *tx).await.map_err(storage::error)?;
    sqlx::query("UPDATE master_code_sequence SET next_value=?1,exhausted=?2,allocated_count=allocated_count+1 WHERE tenant_id=?3 AND rule_id=?4")
        .bind(if next==maximum{next}else{next+1}).bind(next==maximum).bind(&scope.tenant_id).bind(&scope.id)
        .execute(&mut *tx).await.map_err(storage::error)?;
    let row=sqlx::query("SELECT id,tenant_id,rule_id,rule_version,model_id,actor_id,idempotency_key,sequence,code,created_at FROM master_code_allocation WHERE tenant_id=?1 AND id=?2")
        .bind(&scope.tenant_id).bind(id).fetch_one(&mut *tx).await.map_err(storage::error)?;
    let allocation = decode(&row)?;
    tx.commit().await.map_err(storage::error)?;
    Ok(allocation)
}

fn decode(row: &sqlx::sqlite::SqliteRow) -> Result<MasterCodeAllocation, BackendError> {
    let date: String = row.try_get("created_at").map_err(storage::error)?;
    Ok(MasterCodeAllocation {
        id: row.try_get("id").map_err(storage::error)?,
        tenant_id: row.try_get("tenant_id").map_err(storage::error)?,
        rule_id: row.try_get("rule_id").map_err(storage::error)?,
        rule_version: row.try_get("rule_version").map_err(storage::error)?,
        model_id: row.try_get("model_id").map_err(storage::error)?,
        actor_id: row.try_get("actor_id").map_err(storage::error)?,
        idempotency_key: row.try_get("idempotency_key").map_err(storage::error)?,
        sequence: row.try_get("sequence").map_err(storage::error)?,
        code: row.try_get("code").map_err(storage::error)?,
        created_at: DateTime::parse_from_rfc3339(&date)
            .map(|date| date.with_timezone(&Utc))
            .map_err(|e| BackendError::Internal(e.to_string()))?,
    })
}
