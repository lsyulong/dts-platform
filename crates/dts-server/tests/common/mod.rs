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


//! Integration test shared utilities
//!
//! Provides backend initialization, HTTP test client helpers, and JWT token generation.

use dts_server::config::AppConfig;
use dts_server::db::{self, BackendContext};
use dts_server::services::token::Claims;

/// Create a test backend using SQLite in-memory (includes migrations + seed data).
pub async fn setup_test_backend() -> anyhow::Result<BackendContext> {
    // SQLite in-memory: each call creates a fresh database since
    // `init_backend` creates a new pool each time.
    // Use shared cache so the pool's connections see the same data.
    db::init_backend("sqlite::memory:?cache=shared", "sqlite").await
}

/// Create a test AppConfig (SQLite memory).
#[allow(dead_code)]
pub fn test_config() -> AppConfig {
    AppConfig::for_test()
}

/// The JWT secret used by `get_jwt_secret(None)` fallback in handlers
/// that don't have access to AppConfig.
pub const DEFAULT_JWT_SECRET: &str = "dev-secret-change-me";

/// Generate a test JWT token (used by test files).
#[allow(dead_code)]
pub fn generate_test_token(secret: &str, user_id: &str, tenant_id: &str, username: &str) -> String {
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        tenant_id: tenant_id.to_string(),
        username: username.to_string(),
        exp: (now + chrono::Duration::hours(24)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    dts_server::services::token::create_token(&claims, secret)
        .expect("Test token generation failed")
}
