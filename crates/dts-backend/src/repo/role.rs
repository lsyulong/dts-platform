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
use crate::types::{CreateRoleParams, RoleListInfo, UpdateRoleParams};

/// 角色与权限数据访问 trait
#[async_trait]
pub trait RoleRepo: Send + Sync {
    /// 创建角色及其权限
    async fn create_with_permissions(
        &self,
        params: &CreateRoleParams,
    ) -> Result<String, BackendError>;

    /// 获取用户的所有角色名称
    async fn find_user_role_names(&self, user_id: &str) -> Result<Vec<String>, BackendError>;

    /// 获取用户的所有权限
    async fn find_user_permissions(&self, user_id: &str) -> Result<Vec<String>, BackendError>;

    /// 创建默认三角色（tenant_admin, developer, viewer）
    async fn create_default_roles(&self, tenant_id: &str) -> Result<DefaultRoles, BackendError>;

    /// 列出所有角色（支持按租户筛选）
    async fn list_all(&self, tenant_id: Option<&str>) -> Result<Vec<RoleListInfo>, BackendError>;

    /// 更新角色及其权限
    async fn update(&self, role_id: &str, params: &UpdateRoleParams) -> Result<(), BackendError>;

    /// 删除角色（级联删除关联）
    async fn delete(&self, role_id: &str) -> Result<(), BackendError>;

    /// 获取所有可用的权限码列表
    async fn find_all_permissions(&self) -> Result<Vec<String>, BackendError>;
}

/// 默认三角色的 ID
#[derive(Debug, Clone)]
pub struct DefaultRoles {
    pub admin_role_id: String,
    pub developer_role_id: String,
    pub viewer_role_id: String,
}
