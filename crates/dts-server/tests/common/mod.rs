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

//! Integration utilities: isolated SQLite and a shared real Sa-Token manager.

use std::ops::Deref;
use std::time::Duration;

use dts_server::config::AppConfig;
use dts_server::db::{self, BackendContext};
use dts_server::services::{auth_svc::AuthService, sa_auth};
use sa_token_core::TokenValue;

pub struct TestBackend {
    pub ctx: BackendContext,
    pub auth: AuthService,
}

impl Deref for TestBackend {
    type Target = BackendContext;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

/// Every fixture owns its manager; App and token helpers clone this same instance.
pub async fn setup_test_backend() -> anyhow::Result<TestBackend> {
    let ctx = db::init_backend("sqlite::memory:?cache=shared", "sqlite").await?;
    let manager = sa_auth::test_manager(ctx.role_repo.clone());
    let auth = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        manager,
    );
    Ok(TestBackend { ctx, auth })
}

#[allow(dead_code)]
pub fn test_config() -> AppConfig {
    AppConfig::for_test()
}

#[allow(dead_code)]
pub fn test_auth_service(ctx: &TestBackend) -> AuthService {
    ctx.auth.clone()
}

/// Issue a server-side session for an existing account using production metadata.
#[allow(dead_code)]
pub async fn test_token(ctx: &TestBackend, user_id: &str) -> String {
    ctx.auth
        .issue_session_for_user(user_id)
        .await
        .expect("Sa-Token fixture session")
}

#[allow(dead_code)]
pub async fn admin_token(ctx: &TestBackend) -> String {
    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .unwrap()
        .unwrap();
    let user = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .unwrap()
        .unwrap();
    test_token(ctx, &user.id).await
}

/// Exercise actual storage expiry, after proving the same token initially works.
#[allow(dead_code)]
pub async fn expired_admin_token(ctx: &TestBackend) -> String {
    let token = admin_token(ctx).await;
    ctx.auth
        .manager
        .renew_timeout(&TokenValue::new(token.clone()), 1)
        .await
        .expect("Set real Sa-Token expiry");
    assert!(
        ctx.auth.authenticate(&token).await.is_ok(),
        "Expiry fixture must start as a valid session"
    );
    tokio::time::sleep(Duration::from_millis(1200)).await;
    token
}
