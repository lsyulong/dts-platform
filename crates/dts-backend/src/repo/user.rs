use async_trait::async_trait;

use crate::error::BackendError;
use crate::types::{CreateUserParams, UserRecord};

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
    async fn find_detail_by_id(
        &self,
        user_id: &str,
    ) -> Result<Option<UserDetail>, BackendError>;

    /// 给用户分配角色
    async fn assign_role(&self, user_id: &str, role_id: &str) -> Result<(), BackendError>;
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
