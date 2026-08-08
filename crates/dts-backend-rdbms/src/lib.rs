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


//! DTS Backend RDBMS — 关系型数据库公共层
//!
//! 提供所有关系型数据库后端共用的工具：
//! - 错误映射（sqlx Error → BackendError）
//! - 迁移执行器 trait
//! - 通用连接池配置

pub mod error;
pub mod migrate;

pub use error::map_sqlx_error;
pub use migrate::MigrationRunner;
