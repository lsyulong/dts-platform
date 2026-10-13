use super::super::product_resource::SqliteProductResourceRepo as Resources;
use super::storage::{self, ReceiptWrite, conflict, error, invalid};
use dts_backend::{BackendError, repo::navigation::*};
use serde_json::Value;
use sqlx::SqliteConnection;

async fn template(
    conn: &mut SqliteConnection,
    p: &AssignNavigationParams,
) -> Result<NavigationTemplateSnapshot, BackendError> {
    let source =
        Resources::read_resource(conn, &p.scope.tenant_id, "menu_template", &p.template_id)
            .await?
            .ok_or_else(|| BackendError::NotFound("导航模板不属于当前租户或不存在".into()))?;
    if source.version != p.expected_template_version {
        return Err(conflict("模板定义版本已改变"));
    }
    let ids: Vec<String> = serde_json::from_value(source.properties["menu_ids"].clone())
        .map_err(|_| invalid("模板菜单ID数组无效"))?;
    if ids.is_empty() || ids.len() > 1000 {
        return Err(invalid("模板菜单数量必须为1至1000"));
    }
    let mut menus = Vec::new();
    let mut budget = source.properties.to_string().len();
    for id in &ids {
        validation::bounded(id, 128)?;
        let menu = Resources::read_resource(conn, &p.scope.tenant_id, "system_menu", id)
            .await?
            .ok_or_else(|| BackendError::NotFound("模板菜单不属于当前租户或不存在".into()))?;
        budget += menu.properties.to_string().len();
        if budget > 8 * 1024 * 1024 {
            return Err(invalid("模板实际定义超过8MiB读取容量"));
        }
        menus.push(menu);
    }
    validation::freeze(&p.scope.tenant_id, &source, &menus, &p.registry)
}
fn compare(previous: Option<&NavigationAssignment>, version: i64) -> Result<(), BackendError> {
    if previous.map_or(0, |value| value.version) != version {
        return Err(conflict("导航分配版本已经变化"));
    }
    Ok(())
}
pub(super) async fn assign(
    conn: &mut SqliteConnection,
    p: &AssignNavigationParams,
    semantics: &Value,
) -> Result<NavigationDecisionResult, BackendError> {
    if let Some(result) = storage::replay(conn, &p.scope, &p.idempotency_key, semantics).await? {
        return Ok(result);
    }
    let previous = storage::load(conn, &p.scope.tenant_id, true).await?;
    compare(previous.as_ref(), p.expected_assignment_version)?;
    let template = template(conn, p).await?;
    let observed = storage::now(conn).await?;
    let expires_at = validation::deadline(observed, template.validity_days, p.expires_at)?;

    let assignment = NavigationAssignment {
        id: previous
            .as_ref()
            .map(|value| value.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        tenant_id: p.scope.tenant_id.clone(),
        version: p.expected_assignment_version + 1,
        status: NavigationAssignmentStatus::Active,
        template,
        assigned_by: p.scope.actor_id.clone(),
        assigned_at: observed,
        expires_at,
        revoked_by: None,
        revoked_at: None,
    };
    save(conn, &assignment, p.expected_assignment_version).await?;
    let saved = storage::load(conn, &p.scope.tenant_id, true)
        .await?
        .ok_or_else(|| conflict("导航分配未实际保存"))?;
    storage::receipt(
        conn,
        ReceiptWrite {
            scope: &p.scope,
            key: &p.idempotency_key,
            action: "assign",
            semantics,
            assignment: saved,
        },
    )
    .await
}
pub(super) async fn revoke(
    conn: &mut SqliteConnection,
    p: &RevokeNavigationParams,
    semantics: &Value,
) -> Result<NavigationDecisionResult, BackendError> {
    if let Some(result) = storage::replay(conn, &p.scope, &p.idempotency_key, semantics).await? {
        return Ok(result);
    }
    let previous = storage::load(conn, &p.scope.tenant_id, true)
        .await?
        .ok_or_else(|| BackendError::NotFound("真实导航分配不存在".into()))?;
    compare(Some(&previous), p.expected_assignment_version)?;
    if previous.id != p.assignment_id || previous.status == NavigationAssignmentStatus::Revoked {
        return Err(conflict("真实导航分配身份不同或已经撤销"));
    }
    let assignment = NavigationAssignment {
        version: previous.version + 1,
        status: NavigationAssignmentStatus::Revoked,
        revoked_by: Some(p.scope.actor_id.clone()),
        revoked_at: Some(storage::now(conn).await?),
        ..previous
    };
    save(conn, &assignment, p.expected_assignment_version).await?;
    let saved = storage::load(conn, &p.scope.tenant_id, true)
        .await?
        .ok_or_else(|| conflict("导航撤销未实际保存"))?;
    storage::receipt(
        conn,
        ReceiptWrite {
            scope: &p.scope,
            key: &p.idempotency_key,
            action: "revoke",
            semantics,
            assignment: saved,
        },
    )
    .await
}
async fn save(
    conn: &mut SqliteConnection,
    assignment: &NavigationAssignment,
    expected: i64,
) -> Result<(), BackendError> {
    let encoded =
        serde_json::to_string(&assignment.template).map_err(|_| invalid("导航模板快照无法编码"))?;
    if encoded.len() > 2 * 1024 * 1024 {
        return Err(invalid("导航模板快照超过2MiB"));
    }
    let changed=sqlx::query("INSERT INTO tenant_navigation_assignment(tenant_id,id,version,status,template_id,template_version,template_snapshot,
        assigned_by,assigned_at,expires_at,revoked_by,revoked_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
        ON CONFLICT(tenant_id) DO UPDATE SET version=excluded.version,status=excluded.status,template_id=excluded.template_id,
        template_version=excluded.template_version,template_snapshot=excluded.template_snapshot,assigned_by=excluded.assigned_by,
        assigned_at=excluded.assigned_at,expires_at=excluded.expires_at,revoked_by=excluded.revoked_by,revoked_at=excluded.revoked_at
        WHERE tenant_navigation_assignment.version=?13")
        .bind(&assignment.tenant_id).bind(&assignment.id).bind(assignment.version).bind(assignment.status.as_str())
        .bind(&assignment.template.id).bind(assignment.template.version).bind(encoded).bind(&assignment.assigned_by)
        .bind(assignment.assigned_at.to_rfc3339())
        .bind(assignment.expires_at.map(|value|value.to_rfc3339()))
        .bind(&assignment.revoked_by).bind(assignment.revoked_at.map(|value|value.to_rfc3339()))
        .bind(expected).execute(conn).await.map_err(error)?.rows_affected();
    if changed != 1 {
        return Err(conflict("导航分配发生并发变化"));
    }
    Ok(())
}
