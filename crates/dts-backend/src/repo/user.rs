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
use crate::types::{CreateUserParams, UpdateUserParams, UserListInfo, UserRecord};

/// 用户数据访问 trait
#[async_trait]
pub trait UserRepo: Send + Sync {
    /// 创建用户
    async fn create(&self, params: &CreateUserParams) -> Result<String, BackendError>;

    /// 根据租户 ID 和用户名查找用户（含完整字段）
    async fn find_by_tenant_and_username(
        &self,
        tenant_id: &str,
        username: &str,
    ) -> Result<Option<UserRecord>, BackendError>;

    /// 根据 ID 查找用户基本信息（用于 /me 接口）
    async fn find_detail_by_id(&self, user_id: &str) -> Result<Option<UserDetail>, BackendError>;

    /// 给用户分配角色
    async fn assign_role(&self, user_id: &str, role_id: &str) -> Result<(), BackendError>;

    /// 列出所有用户（支持可选筛选）
    async fn list_all(
        &self,
        tenant_id: Option<&str>,
        search: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<UserListInfo>, BackendError>;

    async fn list_references(
        &self,
        _tenant_id: &str,
        _search: &str,
        _limit: u32,
        _offset: u32,
    ) -> Result<UserReferencePage, BackendError> {
        Err(BackendError::Configuration(
            "当前仓储未实现分页用户目录".into(),
        ))
    }

    /// 更新用户基本信息
    async fn update(&self, user_id: &str, params: &UpdateUserParams) -> Result<(), BackendError>;

    /// 软删除用户（设置 status = 'deleted'）
    async fn soft_delete(&self, user_id: &str) -> Result<(), BackendError>;

    /// 重置用户密码
    async fn reset_password(&self, user_id: &str, password_hash: &str) -> Result<(), BackendError>;

    async fn update_self_profile(
        &self,
        _user_id: &str,
        _tenant_id: &str,
        _email: &str,
        _display_name: &str,
    ) -> Result<bool, BackendError> {
        Err(BackendError::Configuration(
            "当前仓储未实现本人资料更新".into(),
        ))
    }

    async fn compare_exchange_password(
        &self,
        _user_id: &str,
        _tenant_id: &str,
        _expected_hash: &str,
        _new_hash: &str,
    ) -> Result<bool, BackendError> {
        Err(BackendError::Configuration(
            "当前仓储未实现原子密码变更".into(),
        ))
    }

    /// 移除用户的所有角色
    async fn remove_roles(&self, user_id: &str) -> Result<(), BackendError>;
}

/// 用户详细信息（关联租户名）
#[derive(Debug, Clone, serde::Serialize)]
pub struct UserDetail {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub tenant_id: String,
    pub tenant_name: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserReferenceItem {
    pub id: String,
    pub tenant_id: String,
    pub username: String,
    pub display_name: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct UserReferencePage {
    pub data: Vec<UserReferenceItem>,
    pub total: i64,
}
