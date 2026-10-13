use super::{
    super::product_resource::{SqliteProductResourceRepo, helpers},
    storage,
};
use dts_backend::{BackendError, ProductResource, repo::organization_members::*};
use serde_json::Value;
use sqlx::SqliteConnection;

async fn change(
    conn: &mut SqliteConnection,
    s: &OrganizationScope,
    org: &ProductResource,
    next: Option<&str>,
) -> Result<OrganizationManagerChange, BackendError> {
    let previous = storage::manager_id(conn, &s.tenant_id, &org.id).await?;
    if previous.as_deref() == next {
        return Err(storage::conflict("组织负责人未发生变化"));
    }
    let user = if let Some(id) = next {
        storage::available(conn, &s.tenant_id, org).await?;
        let user = storage::user(conn, &s.tenant_id, id, true).await?;
        let membership = storage::membership(conn, &s.tenant_id, id)
            .await?
            .ok_or_else(|| storage::conflict("继任负责人必须是本组织实际成员"))?;
        if membership.organization_id.as_deref() != Some(&org.id) {
            return Err(storage::conflict("继任负责人必须是本组织实际成员"));
        }
        Some(user)
    } else {
        None
    };
    let version = org
        .version
        .checked_add(1)
        .ok_or_else(|| storage::conflict("组织版本已达上限"))?;
    sqlx::query("DELETE FROM organization_manager WHERE tenant_id=?1 AND organization_id=?2")
        .bind(&s.tenant_id)
        .bind(&org.id)
        .execute(&mut *conn)
        .await
        .map_err(storage::error)?;
    if let Some(user) = &user {
        sqlx::query("INSERT INTO organization_manager(tenant_id,organization_id,manager_user_id,manager_status) VALUES(?1,?2,?3,?4)")
            .bind(&s.tenant_id).bind(&org.id).bind(&user.id).bind(&user.status).execute(&mut *conn).await.map_err(storage::error)?;
    }
    let rows=sqlx::query("UPDATE product_resource SET properties=json_remove(properties,'$.member_ids','$.manager_user_id'),version=?1,updated_at=?2 WHERE tenant_id=?3 AND kind='organization' AND id=?4 AND version=?5")
        .bind(version).bind(storage::now(conn).await?.to_rfc3339()).bind(&s.tenant_id).bind(&org.id).bind(org.version).execute(&mut *conn).await.map_err(storage::error)?.rows_affected();
    if rows != 1 {
        return Err(storage::conflict("组织已变更，请刷新后重试"));
    }
    sqlx::query("DELETE FROM product_resource_external_links WHERE tenant_id=?1 AND source_kind='organization' AND source_id=?2 AND field IN('member_ids','manager_user_id')")
        .bind(&s.tenant_id).bind(&org.id).execute(&mut *conn).await.map_err(storage::error)?;
    let after =
        SqliteProductResourceRepo::read_resource(conn, &s.tenant_id, "organization", &org.id)
            .await?
            .ok_or_else(|| storage::invalid("负责人提交后组织丢失"))?;
    helpers::audit(
        conn,
        &s.actor_id,
        "organization_manager_change",
        Some(org),
        Some(&after),
    )
    .await?;
    Ok(OrganizationManagerChange {
        organization: (&after).into(),
        previous_user_id: previous,
        manager: user,
    })
}
pub(super) async fn handover(
    conn: &mut SqliteConnection,
    s: &OrganizationScope,
    user: &str,
    source: Option<&ProductResource>,
    handover: Option<&ManagerHandover>,
) -> Result<Vec<OrganizationManagerChange>, BackendError> {
    let Some(org) = source else {
        if handover.is_some() {
            return Err(storage::invalid("无原组织的成员不能指定负责人继任"));
        }
        return Ok(vec![]);
    };
    let manager = storage::manager_id(conn, &s.tenant_id, &org.id).await?;
    if manager.as_deref() != Some(user) {
        if handover.is_some() {
            return Err(storage::invalid("普通成员调动不能修改组织负责人"));
        }
        return Ok(vec![]);
    }
    let h =
        handover.ok_or_else(|| storage::conflict("现任负责人调动或移除必须指定继任或明确卸任"))?;
    if h.expected_organization_version != org.version {
        return Err(storage::conflict("原组织已变更，请刷新继任信息"));
    }
    if h.successor_user_id.as_deref() == Some(user) {
        return Err(storage::invalid("继任负责人不能是正在离开的成员"));
    }
    Ok(vec![
        change(conn, s, org, h.successor_user_id.as_deref()).await?,
    ])
}
pub(super) async fn assign(
    conn: &mut SqliteConnection,
    p: &SetOrganizationManagerParams,
    semantics: &Value,
) -> Result<OrganizationDecisionResult, BackendError> {
    let org = storage::organization(conn, &p.scope.tenant_id, &p.organization_id).await?;
    if org.version != p.expected_organization_version {
        return Err(storage::conflict("组织已变更，请刷新后重试"));
    }
    let change = change(conn, &p.scope, &org, p.manager_user_id.as_deref()).await?;
    let affected = p
        .manager_user_id
        .as_deref()
        .or(change.previous_user_id.as_deref());
    storage::receipt(
        conn,
        storage::ReceiptWrite {
            scope: &p.scope,
            key: &p.idempotency_key,
            action: "set_manager",
            reason: &p.reason,
            user_id: affected,
            semantics,
            result: OrganizationDecisionResult {
                membership: None,
                source: Some((&org).into()),
                target: Some(change.organization.clone()),
                manager_changes: vec![change.clone()],
                receipt_id: uuid::Uuid::new_v4().to_string(),
                replayed: false,
                roles_unchanged: true,
            },
        },
    )
    .await
}
