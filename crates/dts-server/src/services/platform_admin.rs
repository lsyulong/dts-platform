// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::types::{CreateRoleParams, RoleListInfo};

use crate::db::BackendContext;

const ROLE_NAME: &str = "platform_admin";

/// Offline operator action; user selection is explicit and passwords remain untouched.
pub async fn grant(ctx: &BackendContext, user_id: &str) -> anyhow::Result<String> {
    let detail = ctx
        .user_repo
        .find_detail_by_id(user_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("指定用户不存在"))?;
    let user = ctx
        .user_repo
        .find_by_tenant_and_username(&detail.tenant_id, &detail.username)
        .await?
        .filter(|user| user.id == user_id && user.status == "active")
        .ok_or_else(|| anyhow::anyhow!("指定用户不是 active 状态"))?;
    ctx.tenant_repo
        .find_by_id(&user.tenant_id)
        .await?
        .filter(|tenant| tenant.status == "active")
        .ok_or_else(|| anyhow::anyhow!("指定用户的租户不是 active 状态"))?;

    let role_id = platform_role(ctx, &user.tenant_id).await?;
    let assigned = ctx.role_repo.find_user_role_names(&user.id).await?;
    if !assigned.iter().any(|name| name == ROLE_NAME) {
        ctx.user_repo.assign_role(&user.id, &role_id).await?;
    }
    let permissions = ctx.role_repo.find_user_permissions(&user.id).await?;
    if dts_backend::PERMISSIONS
        .iter()
        .any(|required| !permissions.iter().any(|p| p.as_str() == *required))
    {
        anyhow::bail!("平台管理员授权后权限校验失败");
    }
    Ok(role_id)
}

async fn platform_role(ctx: &BackendContext, tenant_id: &str) -> anyhow::Result<String> {
    let roles: Vec<_> = ctx
        .role_repo
        .list_all(Some(tenant_id))
        .await?
        .into_iter()
        .filter(|role| role.name == ROLE_NAME)
        .collect();
    match roles.as_slice() {
        [] => ctx
            .role_repo
            .create_with_permissions(&CreateRoleParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: tenant_id.into(),
                name: ROLE_NAME.into(),
                description: "平台所有者".into(),
                is_system: true,
                permissions: dts_backend::PERMISSIONS
                    .iter()
                    .map(|p| (*p).into())
                    .collect(),
            })
            .await
            .map_err(Into::into),
        [role] => {
            validate_role(role, tenant_id)?;
            Ok(role.id.clone())
        },
        _ => anyhow::bail!("存在多个同名 platform_admin 角色，请先核查数据库"),
    }
}

fn validate_role(role: &RoleListInfo, tenant_id: &str) -> anyhow::Result<()> {
    if role.tenant_id != tenant_id
        || !role.is_system
        || role.permissions.len() != dts_backend::PERMISSIONS.len()
        || dts_backend::PERMISSIONS
            .iter()
            .any(|required| !role.permissions.iter().any(|p| p.as_str() == *required))
    {
        anyhow::bail!("同名 platform_admin 角色不是具有完整 canonical 权限的系统角色，拒绝复用");
    }
    Ok(())
}
