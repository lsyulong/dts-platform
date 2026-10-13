// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use argon2::{Argon2, PasswordVerifier, password_hash::PasswordHash};
use dts_backend::repo::{RoleRepo, TenantRepo, UserRepo};
use dts_backend_sqlite::{SqliteRoleRepo, SqliteTenantRepo, SqliteUserRepo};

use super::*;
use crate::services::{auth_svc::AuthService, tenant_svc::TenantService};

const INITIAL_PASSWORD: &str = "synthetic-initial-admin-password";

fn database_url(directory: &tempfile::TempDir, filename: &str) -> String {
    format!(
        "sqlite://{}?mode=rwc",
        directory.path().join(filename).to_string_lossy()
    )
}

#[tokio::test]
async fn production_bootstrap_rejects_missing_and_short_passwords_before_seed_writes() {
    let directory = tempfile::tempdir().unwrap();
    for (filename, password) in [
        ("missing.db", None),
        ("empty.db", Some("")),
        ("short.db", Some("too-short")),
    ] {
        let url = database_url(&directory, filename);
        let error = init_backend_with_bootstrap(&url, "sqlite", password)
            .await
            .err()
            .expect("Empty production database must require an explicit password");
        assert!(error.to_string().contains("BOOTSTRAP_ADMIN_PASSWORD"));

        let pool = dts_backend_sqlite::create_pool(&url).await.unwrap();
        let tenants = SqliteTenantRepo::new(pool.inner().clone());
        let users = SqliteUserRepo::new(pool.inner().clone());
        let roles = SqliteRoleRepo::new(pool.inner().clone());
        assert_eq!(
            tenants.count().await.unwrap(),
            0,
            "Rejected bootstrap must not insert a tenant"
        );
        assert!(users.list_all(None, None, None).await.unwrap().is_empty());
        assert!(roles.list_all(None).await.unwrap().is_empty());
        pool.inner().close().await;
    }
}

#[tokio::test]
async fn production_bootstrap_stores_the_supplied_argon_password_without_demo_sources() {
    let ctx = init_backend_with_bootstrap(
        "sqlite::memory:?cache=shared",
        "sqlite",
        Some(INITIAL_PASSWORD),
    )
    .await
    .unwrap();
    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .unwrap()
        .unwrap();
    let admin = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .unwrap()
        .unwrap();
    let password_hash = PasswordHash::new(&admin.password_hash).unwrap();
    assert!(
        Argon2::default()
            .verify_password(INITIAL_PASSWORD.as_bytes(), &password_hash)
            .is_ok()
    );
    assert!(
        Argon2::default()
            .verify_password(b"admin123", &password_hash)
            .is_err()
    );
    assert_eq!(
        ctx.datasource_repo
            .count_by_tenant(&tenant.id)
            .await
            .unwrap(),
        0
    );

    let permissions = ctx
        .role_repo
        .find_user_permissions(&admin.id)
        .await
        .unwrap();
    assert!(
        permissions
            .iter()
            .any(|permission| permission == "tenant:create")
    );
    let role_names = ctx.role_repo.find_user_role_names(&admin.id).await.unwrap();
    assert_eq!(role_names, vec!["platform_admin"]);
    let role = ctx
        .role_repo
        .list_all(Some(&tenant.id))
        .await
        .unwrap()
        .into_iter()
        .find(|role| role.name == "platform_admin")
        .unwrap();
    assert!(role.is_system);
}

#[tokio::test]
async fn production_restart_never_resets_an_existing_administrator_password() {
    let directory = tempfile::tempdir().unwrap();
    let url = database_url(&directory, "restart.db");
    let first = init_backend_with_bootstrap(&url, "sqlite", Some(INITIAL_PASSWORD))
        .await
        .unwrap();
    let tenant = first
        .tenant_repo
        .find_by_slug("default")
        .await
        .unwrap()
        .unwrap();
    let original = first
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .unwrap()
        .unwrap();
    drop(first);

    for password in [None, Some("short"), Some("different-bootstrap-password")] {
        let restarted = init_backend_with_bootstrap(&url, "sqlite", password)
            .await
            .unwrap();
        let admin = restarted
            .user_repo
            .find_by_tenant_and_username(&tenant.id, "admin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(admin.id, original.id);
        assert_eq!(
            admin.password_hash, original.password_hash,
            "Bootstrap password configuration must never reset an existing account"
        );
        assert_eq!(restarted.tenant_repo.count().await.unwrap(), 1);
    }
}

#[tokio::test]
async fn platform_bootstrap_does_not_give_new_tenant_administrators_platform_permissions() {
    let ctx = init_backend_with_bootstrap(
        "sqlite::memory:?cache=shared",
        "sqlite",
        Some(INITIAL_PASSWORD),
    )
    .await
    .unwrap();
    let service = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );
    let password_hash = AuthService::hash_password_sync("synthetic-tenant-admin-password").unwrap();
    let tenant_id = service
        .create_tenant(
            "Customer",
            "customer",
            "customer-admin",
            "customer-admin@example.invalid",
            &password_hash,
        )
        .await
        .unwrap();
    let admin = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant_id, "customer-admin")
        .await
        .unwrap()
        .unwrap();
    let permissions = ctx
        .role_repo
        .find_user_permissions(&admin.id)
        .await
        .unwrap();
    assert!(
        permissions
            .iter()
            .any(|permission| permission == "user:manage")
    );
    assert!(
        !permissions
            .iter()
            .any(|permission| permission == "tenant:create")
    );
    assert_eq!(
        ctx.role_repo.find_user_role_names(&admin.id).await.unwrap(),
        vec!["tenant_admin"]
    );
}

#[tokio::test]
async fn production_seeder_also_rejects_short_password_when_called_directly() {
    let pool = dts_backend_sqlite::create_pool("sqlite::memory:?cache=shared")
        .await
        .unwrap();
    dts_backend_sqlite::SqliteMigrationRunner::new(pool.inner().clone())
        .run_migrations()
        .await
        .unwrap();
    let seed = dts_backend_sqlite::SqliteSeedRepo::new(pool.inner().clone());
    assert!(seed.seed_bootstrap("short").await.is_err());
    assert!(!seed.has_data().await.unwrap());
    pool.inner().close().await;
}
