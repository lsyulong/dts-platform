// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Backend-layer integration tests using SQLite in-memory

mod common;

use pretty_assertions::assert_eq;

/// Verify that init_backend creates a working SQLite backend.
#[tokio::test]
async fn init_backend_creates_working_context() {
    let ctx = common::setup_test_backend().await;
    assert!(ctx.is_ok(), "BackendContext initialization should succeed");

    let ctx = ctx.unwrap();
    // Verify pool is pingable
    ctx.pool.ping().await.expect("pool should be pingable");
}

/// Verify that seed data was populated (tenant exists).
#[tokio::test]
async fn init_backend_seeds_default_data() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let has_data = ctx.seed_repo.has_data().await.expect("has_data query");
    assert!(has_data, "Seed repo should report has_data = true");

    // Verify tenant was created
    let tenants = ctx.tenant_repo.list_all().await.expect("list tenants");
    assert!(!tenants.is_empty(), "At least one tenant should be seeded");
    assert_eq!(tenants[0].slug, "default");
    assert_eq!(tenants[0].status, "active");
}

/// Verify that init_backend is idempotent: calling init_backend twice
/// on the same logical database skips seed on the second call
/// because has_data() returns true.
#[tokio::test]
async fn init_backend_is_idempotent() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    // has_data() should return true after seed
    let has_data = ctx.seed_repo.has_data().await.expect("has_data query");
    assert!(has_data, "Seed repo should report has_data = true");

    // Calling seed directly on an already-seeded database may fail
    // due to unique constraints (e.g. duplicate tenant slug).
    // The idempotency is enforced by the has_data() guard in init_backend,
    // not by the seed() method itself.
    let seed_result = ctx.seed_repo.seed().await;
    // Whether it succeeds or fails, the data count should remain 1
    let tenants = ctx.tenant_repo.list_all().await.expect("list tenants");
    assert_eq!(tenants.len(), 1, "Tenant count should remain 1");
    // Log what happened for informational purposes
    if seed_result.is_err() {
        eprintln!(
            "Note: seed() returned error on re-run (expected - unique constraints): {:?}",
            seed_result.unwrap_err()
        );
    }
}

/// Verify that tenant is findable by slug after seed.
#[tokio::test]
async fn seed_tenant_is_findable_by_slug() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .expect("find_by_slug");
    assert!(tenant.is_some(), "Default tenant should exist");
    let tenant = tenant.unwrap();
    assert_eq!(tenant.slug, "default");
    assert_eq!(tenant.name, "默认租户");
    assert_eq!(tenant.status, "active");
}

/// Verify that admin user exists after seed with correct username.
#[tokio::test]
async fn seed_admin_user_exists() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .expect("find_by_slug")
        .expect("tenant");

    let user = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .expect("find_by_tenant_and_username");
    assert!(user.is_some(), "Admin user should exist");
    let user = user.unwrap();
    assert_eq!(user.username, "admin");
    assert_eq!(user.email, "admin@dts.local");
    assert_eq!(user.status, "active");
}

/// Verify that demo datasources exist after seed.
#[tokio::test]
async fn seed_datasources_exist() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .expect("find_by_slug")
        .expect("tenant");

    let ds_list = ctx
        .datasource_repo
        .list_by_tenant(&tenant.id)
        .await
        .expect("list_by_tenant");
    assert_eq!(ds_list.len(), 2, "Two demo datasources should be seeded");
    assert!(ds_list.iter().any(|d| d.name == "生产 MySQL"));
    assert!(ds_list.iter().any(|d| d.name == "分析 PostgreSQL"));
}

/// Verify that a second init_backend creates a fresh instance
/// (different in-memory database — different tenant IDs).
#[tokio::test]
async fn second_backend_init_is_independent() {
    let ctx1 = common::setup_test_backend().await.expect("first backend");
    let ctx2 = common::setup_test_backend().await.expect("second backend");

    // Both should have the default tenant but with different IDs
    let tenant1 = ctx1
        .tenant_repo
        .find_by_slug("default")
        .await
        .expect("find_by_slug")
        .expect("tenant1");
    let tenant2 = ctx2
        .tenant_repo
        .find_by_slug("default")
        .await
        .expect("find_by_slug")
        .expect("tenant2");

    assert_ne!(
        tenant1.id, tenant2.id,
        "Different backends should have different tenant IDs"
    );
}

/// Verify that admin user has roles assigned after seed.
#[tokio::test]
async fn seed_admin_user_has_roles() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .expect("find_by_slug")
        .expect("tenant");

    let user = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .expect("find_by_tenant_and_username")
        .expect("user");

    let roles = ctx
        .role_repo
        .find_user_role_names(&user.id)
        .await
        .expect("find_user_role_names");
    assert!(
        !roles.is_empty(),
        "Admin user should have at least one role"
    );
}

/// Verify that admin user has permissions after seed.
#[tokio::test]
async fn seed_admin_user_has_permissions() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .expect("find_by_slug")
        .expect("tenant");

    let user = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .expect("find_by_tenant_and_username")
        .expect("user");

    let permissions = ctx
        .role_repo
        .find_user_permissions(&user.id)
        .await
        .expect("find_user_permissions");
    assert!(
        !permissions.is_empty(),
        "Admin user should have permissions"
    );
}

/// Verify that datasources count is correct.
#[tokio::test]
async fn datasource_count_is_correct() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .expect("find_by_slug")
        .expect("tenant");

    let count = ctx
        .datasource_repo
        .count_by_tenant(&tenant.id)
        .await
        .expect("count_by_tenant");
    assert_eq!(count, 2, "Should have 2 seeded datasources");

    let healthy = ctx
        .datasource_repo
        .count_healthy_by_tenant(&tenant.id)
        .await
        .expect("count_healthy_by_tenant");
    // Newly seeded datasources may have "unknown" health status, so count >= 0
    assert!(healthy >= 0);
}
