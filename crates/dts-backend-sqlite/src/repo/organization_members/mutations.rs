use super::{manager, storage};
use dts_backend::{BackendError, repo::organization_members::*};
use serde_json::Value;
use sqlx::SqliteConnection;

struct MembershipWrite<'a> {
    scope: &'a OrganizationScope,
    user: &'a OrganizationUser,
    before: Option<&'a OrganizationMembership>,
    target: Option<&'a str>,
}
async fn write(
    conn: &mut SqliteConnection,
    p: MembershipWrite<'_>,
) -> Result<OrganizationMembership, BackendError> {
    let version = p
        .before
        .map_or(0, |m| m.version)
        .checked_add(1)
        .ok_or_else(|| storage::conflict("组织成员版本已达上限"))?;
    let observed = storage::now(conn).await?;
    if let Some(before) = p.before {
        let rows=sqlx::query("UPDATE organization_membership SET organization_id=?1,version=?2,changed_by=?3,changed_at=?4 WHERE tenant_id=?5 AND user_id=?6 AND version=?7")
            .bind(p.target).bind(version).bind(&p.scope.actor_id).bind(observed.to_rfc3339()).bind(&p.scope.tenant_id).bind(&p.user.id).bind(before.version)
            .execute(&mut *conn).await.map_err(storage::error)?.rows_affected();
        if rows != 1 {
            return Err(storage::conflict("组织成员已变更，请刷新后重试"));
        }
    } else {
        sqlx::query("INSERT INTO organization_membership(tenant_id,user_id,user_status,id,version,organization_id,changed_by,changed_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")
            .bind(&p.scope.tenant_id).bind(&p.user.id).bind(&p.user.status).bind(uuid::Uuid::new_v4().to_string()).bind(version).bind(p.target).bind(&p.scope.actor_id)
            .bind(observed.to_rfc3339()).execute(&mut *conn).await.map_err(storage::error)?;
    }
    storage::membership(conn, &p.scope.tenant_id, &p.user.id)
        .await?
        .ok_or_else(|| storage::invalid("组织成员提交后丢失"))
}
pub(super) async fn transfer(
    conn: &mut SqliteConnection,
    p: &TransferOrganizationMemberParams,
    semantics: &Value,
) -> Result<OrganizationDecisionResult, BackendError> {
    let user = storage::user(conn, &p.scope.tenant_id, &p.user_id, true).await?;
    let before = storage::membership(conn, &p.scope.tenant_id, &p.user_id).await?;
    if before.as_ref().map_or(0, |m| m.version) != p.expected_membership_version {
        return Err(storage::conflict("组织成员已变更，请刷新后重试"));
    }
    if before.as_ref().and_then(|m| m.organization_id.as_deref()) == Some(&p.target_organization_id)
    {
        return Err(storage::conflict("成员已属于目标组织"));
    }
    let target = storage::organization(conn, &p.scope.tenant_id, &p.target_organization_id).await?;
    if target.version != p.expected_target_version {
        return Err(storage::conflict("目标组织已变更，请刷新后重试"));
    }
    storage::available(conn, &p.scope.tenant_id, &target).await?;
    let source = match before.as_ref().and_then(|m| m.organization_id.as_deref()) {
        Some(id) => Some(storage::organization(conn, &p.scope.tenant_id, id).await?),
        None => None,
    };
    let changes = manager::handover(
        conn,
        &p.scope,
        &p.user_id,
        source.as_ref(),
        p.manager_handover.as_ref(),
    )
    .await?;
    let membership = write(
        conn,
        MembershipWrite {
            scope: &p.scope,
            user: &user,
            before: before.as_ref(),
            target: Some(&target.id),
        },
    )
    .await?;
    storage::receipt(
        conn,
        storage::ReceiptWrite {
            scope: &p.scope,
            key: &p.idempotency_key,
            action: "transfer",
            reason: &p.reason,
            user_id: Some(&p.user_id),
            semantics,
            result: OrganizationDecisionResult {
                membership: Some(membership),
                source: source.as_ref().map(OrganizationIdentity::from),
                target: Some((&target).into()),
                manager_changes: changes,
                receipt_id: uuid::Uuid::new_v4().to_string(),
                replayed: false,
                roles_unchanged: true,
            },
        },
    )
    .await
}
pub(super) async fn remove(
    conn: &mut SqliteConnection,
    p: &RemoveOrganizationMemberParams,
    semantics: &Value,
) -> Result<OrganizationDecisionResult, BackendError> {
    let user = storage::user(conn, &p.scope.tenant_id, &p.user_id, false).await?;
    let before = storage::membership(conn, &p.scope.tenant_id, &p.user_id)
        .await?
        .ok_or_else(|| storage::conflict("用户没有实际组织归属"))?;
    if before.version != p.expected_membership_version {
        return Err(storage::conflict("组织成员已变更，请刷新后重试"));
    }
    let org = before
        .organization_id
        .as_ref()
        .ok_or_else(|| storage::conflict("用户已无组织归属"))?;
    let source = storage::organization(conn, &p.scope.tenant_id, org).await?;
    let changes = manager::handover(
        conn,
        &p.scope,
        &p.user_id,
        Some(&source),
        p.manager_handover.as_ref(),
    )
    .await?;
    let membership = write(
        conn,
        MembershipWrite {
            scope: &p.scope,
            user: &user,
            before: Some(&before),
            target: None,
        },
    )
    .await?;
    storage::receipt(
        conn,
        storage::ReceiptWrite {
            scope: &p.scope,
            key: &p.idempotency_key,
            action: "remove",
            reason: &p.reason,
            user_id: Some(&p.user_id),
            semantics,
            result: OrganizationDecisionResult {
                membership: Some(membership),
                source: Some((&source).into()),
                target: None,
                manager_changes: changes,
                receipt_id: uuid::Uuid::new_v4().to_string(),
                replayed: false,
                roles_unchanged: true,
            },
        },
    )
    .await
}
