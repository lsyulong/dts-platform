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
/// Provides explicit production bootstrap and legacy development fixtures.
#[async_trait]
pub trait SeedRepo: Send + Sync {
    /// 检查数据库中是否已有数据
    async fn has_data(&self) -> Result<bool, BackendError>;

    /// Development/test fixtures only: includes demonstration sources and credentials.
    async fn seed(&self) -> Result<(), BackendError>;

    /// Bootstrap an empty production database without demonstration sources.
    /// Implementations must validate the password before any data is inserted.
    async fn seed_bootstrap(&self, _admin_password: &str) -> Result<(), BackendError> {
        Err(BackendError::Configuration(
            "当前后端未实现安全管理员初始化".into(),
        ))
    }
}
