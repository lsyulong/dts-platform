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


//! DTS Backend PostgreSQL — PostgreSQL 协议存储后端
//!
//! 基于 sqlx 实现，兼容所有 PostgreSQL 协议数据库：
//! - PostgreSQL
//! - GaussDB / openGauss（华为）
//! - Kingbase（人大金仓）
//! - HighGo（瀚高）
//! - CockroachDB / YugabyteDB
//! - Greenplum / Redshift

pub mod pool;
pub mod repo;

mod migrate;

pub use migrate::PgMigrationRunner;
pub use pool::{PgBackendPool, create_pool};
pub use repo::{
    PgAlertRepo, PgDatasourceRepo, PgMonitoringRepo, PgRoleRepo, PgSeedRepo, PgTaskRepo,
    PgTaskRunRepo, PgTenantRepo, PgUserRepo,
};
