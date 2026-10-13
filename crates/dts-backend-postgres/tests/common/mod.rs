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

//! Shared test utilities for dts-backend-postgres integration tests.
//!
//! Requires a running PostgreSQL instance. Start with:
//! ```bash
//! docker compose -f docker-compose.test.yml up -d
//! ```
//! Or set `DATABASE_URL` to any PostgreSQL-compatible database.

use std::sync::OnceLock;

use dts_backend_postgres::PgMigrationRunner;
use dts_backend_rdbms::MigrationRunner;
use sqlx::PgPool;

/// Get the test database URL from environment or use the default.
pub fn test_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://dts:dts_test@localhost:5433/dts_test".to_string())
}

/// Cached pool for test isolation — each test function gets the same pool,
/// and migrations are run exactly once.
static POOL: OnceLock<Option<PgPool>> = OnceLock::new();

/// Set up a test PostgreSQL pool with migrations applied.
/// Returns `None` if PostgreSQL is not available (tests should skip).
pub async fn setup_pool() -> Option<PgPool> {
    if let Some(pool_opt) = POOL.get() {
        return pool_opt.clone();
    }

    let url = test_database_url();
    match PgPool::connect(&url).await {
        Ok(pool) => {
            let runner = PgMigrationRunner::new(pool.clone());
            if let Err(e) = runner.run_migrations().await {
                eprintln!("Migration failed (may be a shared DB): {e}");
            }
            let pool_clone = pool.clone();
            let _ = POOL.set(Some(pool));
            Some(pool_clone)
        },
        Err(e) => {
            eprintln!("Skipping PostgreSQL tests: {e}");
            eprintln!("Start PostgreSQL with: docker compose -f docker-compose.test.yml up -d");
            let _ = POOL.set(None);
            None
        },
    }
}
