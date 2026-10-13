// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{PgSensitiveFindingRepo, producer::current_sources, storage::*};
use dts_backend::{BackendError, repo::sensitive_findings::*};
use serde_json::{Value, json};
pub(super) async fn decide(
    repo: &PgSensitiveFindingRepo,
    p: &SensitiveReviewParams,
) -> Result<SensitiveFinding, BackendError> {
    validate_review(p)?;
    let mut tx = repo.pool.begin().await.map_err(error)?;
    lock_tenant(&mut tx, &p.tenant_id).await?;
    authority(&mut tx, &p.tenant_id, &p.actor_id, "write").await?;
    let found = required(&mut tx, &p.tenant_id, &p.finding_id).await?;
    if found.status != "pending" || found.version != p.expected_version {
        return Err(invalid("发现已经复核或版本已改变"));
    }
    if p.confirm {
        let text:String=sqlx::query_scalar("SELECT definition_snapshot::text FROM product_job WHERE tenant_id=$1 AND id=$2 AND status='succeeded'").bind(&p.tenant_id).bind(&found.job_id).fetch_one(&mut *tx).await.map_err(error)?;
        let snapshot: Value =
            serde_json::from_str(&text).map_err(|_| invalid("发现的实际运行快照无效"))?;
        if snapshot_hash(&snapshot)? != found.definition_hash {
            return Err(invalid("发现冻结定义摘要不匹配"));
        }
        current_sources(&mut tx, &p.tenant_id, &snapshot).await?;
    }
    let status = if p.confirm { "confirmed" } else { "dismissed" };
    let changed=sqlx::query("UPDATE sensitive_finding SET status=$4,version=version+1,reviewed_by=$5,review_reason=$6,reviewed_at=clock_timestamp() WHERE tenant_id=$1 AND id=$2 AND status='pending' AND version=$3").bind(&p.tenant_id).bind(&p.finding_id).bind(p.expected_version).bind(status).bind(&p.actor_id).bind(&p.reason).execute(&mut *tx).await.map_err(error)?;
    if changed.rows_affected() != 1 {
        return Err(invalid("发现复核发生版本冲突"));
    }
    let changed=sqlx::query("UPDATE product_resource SET status=$4,version=version+1,properties=jsonb_set(properties,'{confirmed}',$5::jsonb),updated_at=clock_timestamp() WHERE tenant_id=$1 AND kind='sensitive_finding' AND id=$2 AND status='pending' AND version=$3").bind(&p.tenant_id).bind(&p.finding_id).bind(p.expected_version).bind(status).bind(if p.confirm{"true"}else{"false"}).execute(&mut *tx).await.map_err(error)?;
    if changed.rows_affected() != 1 {
        return Err(invalid("发现目录投影已变化，复核回滚"));
    }
    audit(
        &mut tx,
        &p.tenant_id,
        &p.finding_id,
        &p.actor_id,
        status,
        Some(&p.reason),
    )
    .await?;
    resource_audit(&mut tx,&p.tenant_id,&p.finding_id,&p.actor_id,"update",json!({"status":status,"version":p.expected_version+1,"reviewed_by":p.actor_id,"classification_enforced":false})).await?;
    let result = required(&mut tx, &p.tenant_id, &p.finding_id).await?;
    tx.commit().await.map_err(error)?;
    Ok(result)
}
