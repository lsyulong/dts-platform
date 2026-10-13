// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "sqlite")]

use dts_backend::types::{CreateRoleParams, UpdateUserParams};
use dts_server::db::{self, BackendContext};
use dts_server::services::{auth_svc::AuthService, platform_admin, tenant_svc::TenantService};

async fn legacy_administrator(ctx: &BackendContext) -> dts_backend::types::UserRecord {
    let hash = AuthService::hash_password_sync("synthetic-existing-admin-password").unwrap();
    let tenant = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    )
    .create_tenant(
        "Legacy",
        "legacy",
        "existing_operator",
        "operator@example.test",
        &hash,
    )
    .await
    .unwrap();
    ctx.user_repo
        .find_by_tenant_and_username(&tenant, "existing_operator")
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn explicit_grant_restores_legacy_tenant_creation_without_changing_password_and_is_idempotent()
 {
    let ctx = db::init_backend_for_command("sqlite::memory:?cache=shared", "sqlite")
        .await
        .unwrap();
    let user = legacy_administrator(&ctx).await;
    assert!(
        !ctx.role_repo
            .find_user_permissions(&user.id)
            .await
            .unwrap()
            .iter()
            .any(|permission| permission == "tenant:create")
    );
    let roles_before = ctx.role_repo.list_all(None).await.unwrap().len();

    let role_id = platform_admin::grant(&ctx, &user.id).await.unwrap();
    assert_eq!(
        platform_admin::grant(&ctx, &user.id).await.unwrap(),
        role_id
    );
    let roles = ctx.role_repo.list_all(None).await.unwrap();
    assert_eq!(roles.len(), roles_before + 1);
    let platform_role = roles.iter().find(|role| role.id == role_id).unwrap();
    assert!(platform_role.is_system);
    assert_eq!(platform_role.tenant_id, user.tenant_id);
    let permissions = ctx.role_repo.find_user_permissions(&user.id).await.unwrap();
    for permission in dts_backend::PERMISSIONS {
        assert!(
            permissions
                .iter()
                .any(|actual| actual.as_str() == *permission),
            "{permission}"
        );
    }
    let roles = ctx.role_repo.find_user_role_names(&user.id).await.unwrap();
    assert_eq!(
        roles
            .iter()
            .filter(|name| *name == "platform_admin")
            .count(),
        1
    );
    assert!(roles.iter().any(|name| name == "tenant_admin"));
    let after = ctx
        .user_repo
        .find_by_tenant_and_username(&user.tenant_id, &user.username)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.password_hash, user.password_hash);
}

#[tokio::test]
async fn recovery_rejects_unknown_or_inactive_accounts_and_same_name_non_system_roles() {
    let ctx = db::init_backend_for_command("sqlite::memory:?cache=shared", "sqlite")
        .await
        .unwrap();
    assert!(
        platform_admin::grant(&ctx, "missing-account-id")
            .await
            .is_err()
    );
    assert!(ctx.role_repo.list_all(None).await.unwrap().is_empty());
    assert!(
        ctx.user_repo
            .list_all(None, None, None)
            .await
            .unwrap()
            .is_empty()
    );

    let user = legacy_administrator(&ctx).await;
    let role_count = ctx.role_repo.list_all(None).await.unwrap().len();
    ctx.user_repo
        .update(
            &user.id,
            &UpdateUserParams {
                display_name: None,
                status: Some("disabled".into()),
            },
        )
        .await
        .unwrap();
    assert!(platform_admin::grant(&ctx, &user.id).await.is_err());
    assert_eq!(
        ctx.role_repo.list_all(None).await.unwrap().len(),
        role_count
    );

    ctx.user_repo
        .update(
            &user.id,
            &UpdateUserParams {
                display_name: None,
                status: Some("active".into()),
            },
        )
        .await
        .unwrap();
    ctx.role_repo
        .create_with_permissions(&CreateRoleParams {
            id: "ordinary-name-collision".into(),
            tenant_id: user.tenant_id.clone(),
            name: "platform_admin".into(),
            description: "Ordinary custom role".into(),
            is_system: false,
            permissions: dts_backend::PERMISSIONS
                .iter()
                .map(|p| (*p).into())
                .collect(),
        })
        .await
        .unwrap();
    assert!(platform_admin::grant(&ctx, &user.id).await.is_err());
    assert_eq!(
        ctx.role_repo.list_all(None).await.unwrap().len(),
        role_count + 1
    );
    assert!(
        !ctx.role_repo
            .find_user_role_names(&user.id)
            .await
            .unwrap()
            .iter()
            .any(|name| name == "platform_admin")
    );
    ctx.role_repo
        .delete("ordinary-name-collision")
        .await
        .unwrap();
    ctx.role_repo
        .create_with_permissions(&CreateRoleParams {
            id: "incomplete-system-role".into(),
            tenant_id: user.tenant_id.clone(),
            name: "platform_admin".into(),
            description: "Incomplete existing system role".into(),
            is_system: true,
            permissions: vec!["task:read".into()],
        })
        .await
        .unwrap();
    assert!(platform_admin::grant(&ctx, &user.id).await.is_err());
    let roles = ctx.role_repo.list_all(None).await.unwrap();
    assert_eq!(roles.len(), role_count + 1);
    assert_eq!(
        roles
            .iter()
            .find(|role| role.id == "incomplete-system-role")
            .unwrap()
            .permissions,
        vec!["task:read".to_owned()]
    );
}
