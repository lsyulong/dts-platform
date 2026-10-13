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

//! DTS Backend — 存储后端核心抽象层
//!
//! 定义所有存储后端必须实现的 trait 和公共类型。
//! 实现者只需遵循 trait 约定即可让 dts-platform 支持新的数据库协议。
//!
//! # 架构
//!
//! ```text
//! dts-server (应用层) → dts-backend (trait 层) → dts-backend-postgres / mysql / oracle ...
//! ```

pub mod error;
pub mod pool;
pub mod repo;

// Re-export types from dts-core (backward compatible)
pub use dts_core::*;

pub use error::BackendError;
pub use pool::{BackendPool, BackendPoolFactory};
pub use repo::product_resource::*;
pub use repo::{
    AlertRepo, CreateTaskRunParams, DatasourceRepo, MonitoringRepo, RoleRepo, SeedRepo, TaskRepo,
    TaskRunRepo, TenantRepo, UpdateTaskRunParams, UserRepo,
};
