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
use crate::types::{CreateTenantParams, TenantInfo, UpdateTenantParams};

/// 租户数据访问 trait
///
/// 所有存储后端实现必须提供此 trait 的具体实现。
#[async_trait]
pub trait TenantRepo: Send + Sync {
    /// 创建租户，返回租户 ID
    async fn create(&self, params: &CreateTenantParams) -> Result<String, BackendError>;

    /// 根据 slug 查找租户
    async fn find_by_slug(&self, slug: &str) -> Result<Option<TenantInfo>, BackendError>;

    /// 根据 ID 查找租户
    async fn find_by_id(&self, id: &str) -> Result<Option<TenantInfo>, BackendError>;

    /// 获取所有租户（按创建时间倒序）
    async fn list_all(&self) -> Result<Vec<TenantInfo>, BackendError>;

    /// 租户总数
    async fn count(&self) -> Result<i64, BackendError>;

    /// 更新租户信息
    async fn update(
        &self,
        tenant_id: &str,
        params: &UpdateTenantParams,
    ) -> Result<(), BackendError>;
}
