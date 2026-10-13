use super::storage;
use dts_backend::{BackendError, ResourceReference, repo::organization_members::*};
use serde_json::json;
use sqlx::SqliteConnection;

pub(super) async fn details(
    conn: &mut SqliteConnection,
    p: &ReadOrganizationParams,
) -> Result<OrganizationDetails, BackendError> {
    let mut organization =
        storage::organization(conn, &p.scope.tenant_id, &p.organization_id).await?;
    let legacy_membership_ignored = organization.properties.get("member_ids").is_some()
        || organization.properties.get("manager_user_id").is_some();
    let id = storage::manager_id(conn, &p.scope.tenant_id, &p.organization_id).await?;
    let manager = if let Some(id) = &id {
        let user = storage::user(conn, &p.scope.tenant_id, id, false).await?;
        let membership = storage::membership(conn, &p.scope.tenant_id, id)
            .await?
            .ok_or_else(|| storage::invalid("负责人实际成员关系丢失"))?;
        Some(OrganizationMember {
            user,
            membership,
            is_manager: true,
        })
    } else {
        None
    };
    let manager_valid = manager.as_ref().is_some_and(|m| {
        m.user.status == "active"
            && m.membership.organization_id.as_deref() == Some(&p.organization_id)
    });
    let member_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM organization_membership WHERE tenant_id=?1 AND organization_id=?2",
    )
    .bind(&p.scope.tenant_id)
    .bind(&p.organization_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(storage::error)?;
    let children_count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM product_resource WHERE tenant_id=?1 AND kind='organization' AND parent_id=?2")
        .bind(&p.scope.tenant_id).bind(&p.organization_id).fetch_one(conn).await.map_err(storage::error)?;
    let props = organization
        .properties
        .as_object_mut()
        .ok_or_else(|| storage::invalid("组织属性损坏"))?;
    props.remove("member_ids");
    props.insert("manager_user_id".into(), json!(id));
    organization
        .references
        .retain(|r| r.field != "member_ids" && r.field != "manager_user_id");
    if let Some(id) = id {
        organization.references.push(ResourceReference {
            field: "manager_user_id".into(),
            target_kind: "user".into(),
            target_id: id,
        });
    }
    Ok(OrganizationDetails {
        organization,
        manager,
        manager_valid,
        member_count,
        children_count,
        legacy_membership_ignored,
    })
}
pub(super) async fn members(
    conn: &mut SqliteConnection,
    p: &OrganizationMemberFilter,
    q: Option<&str>,
) -> Result<OrganizationMemberPage, BackendError> {
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM organization_membership m JOIN \"user\" u ON u.tenant_id=m.tenant_id AND u.id=m.user_id WHERE m.tenant_id=?1 AND m.organization_id=?2 AND (?3 IS NULL OR lower(u.username) LIKE ?3 ESCAPE '\\' OR lower(COALESCE(u.display_name,'')) LIKE ?3 ESCAPE '\\')")
        .bind(&p.scope.tenant_id).bind(&p.organization_id).bind(q).fetch_one(&mut *conn).await.map_err(storage::error)?;
    let manager = storage::manager_id(conn, &p.scope.tenant_id, &p.organization_id).await?;
    let rows=sqlx::query("SELECT m.*,u.username,u.display_name,u.status AS actual_user_status FROM organization_membership m JOIN \"user\" u ON u.tenant_id=m.tenant_id AND u.id=m.user_id WHERE m.tenant_id=?1 AND m.organization_id=?2 AND (?3 IS NULL OR lower(u.username) LIKE ?3 ESCAPE '\\' OR lower(COALESCE(u.display_name,'')) LIKE ?3 ESCAPE '\\') ORDER BY u.username,m.user_id LIMIT ?4 OFFSET ?5")
        .bind(&p.scope.tenant_id).bind(&p.organization_id).bind(q).bind(i64::from(p.limit)).bind(i64::from(p.offset)).fetch_all(conn).await.map_err(storage::error)?;
    let data = rows
        .iter()
        .map(|row| {
            let user = storage::decode_user(row)?;
            let is_manager = manager.as_deref() == Some(&user.id);
            Ok(OrganizationMember {
                user,
                membership: storage::decode_membership(row)?,
                is_manager,
            })
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    Ok(OrganizationMemberPage { data, total })
}
pub(super) async fn history(
    conn: &mut SqliteConnection,
    p: &OrganizationHistoryFilter,
) -> Result<OrganizationHistoryPage, BackendError> {
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM organization_membership_receipt WHERE tenant_id=?1 AND (?2 IS NULL OR source_organization_id=?2 OR target_organization_id=?2) AND (?3 IS NULL OR user_id=?3)")
        .bind(&p.scope.tenant_id).bind(&p.organization_id).bind(&p.user_id).fetch_one(&mut *conn).await.map_err(storage::error)?;
    let rows=sqlx::query("SELECT * FROM organization_membership_receipt WHERE tenant_id=?1 AND (?2 IS NULL OR source_organization_id=?2 OR target_organization_id=?2) AND (?3 IS NULL OR user_id=?3) ORDER BY created_at DESC,id DESC LIMIT ?4 OFFSET ?5")
        .bind(&p.scope.tenant_id).bind(&p.organization_id).bind(&p.user_id).bind(i64::from(p.limit)).bind(i64::from(p.offset)).fetch_all(conn).await.map_err(storage::error)?;
    let data = rows
        .iter()
        .map(storage::decode_history)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OrganizationHistoryPage { data, total })
}
