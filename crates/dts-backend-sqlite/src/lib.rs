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


//! DTS Backend SQLite — SQLite 协议存储后端
//!
//! 基于 sqlx 实现，用于本地开发和测试。
//! 使用 sqlite::memory: 模式可实现零配置测试。

pub mod pool;
pub mod repo;

mod migrate;

pub use migrate::SqliteMigrationRunner;
pub use pool::{SqliteBackendPool, create_pool};
pub use repo::{
    SqliteAlertRepo, SqliteDatasourceRepo, SqliteMonitoringRepo, SqliteRoleRepo, SqliteSeedRepo,
    SqliteTaskRepo, SqliteTaskRunRepo, SqliteTenantRepo, SqliteUserRepo,
};
