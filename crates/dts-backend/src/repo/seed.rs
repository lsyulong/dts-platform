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


use async_trait::async_trait;

use crate::error::BackendError;

/// 种子数据初始化 trait
///
/// 在数据库为空时插入默认租户、角色、管理员用户和演示数据源。
#[async_trait]
pub trait SeedRepo: Send + Sync {
    /// 检查数据库中是否已有数据
    async fn has_data(&self) -> Result<bool, BackendError>;

    /// 执行种子数据初始化
    async fn seed(&self) -> Result<(), BackendError>;
}
