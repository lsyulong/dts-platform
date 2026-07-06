use async_trait::async_trait;

use crate::error::BackendError;
use crate::types::CreateRoleParams;

/// 角色与权限数据访问 trait
#[async_trait]
pub trait RoleRepo: Send + Sync {
    /// 创建角色及其权限
    async fn create_with_permissions(&self, params: &CreateRoleParams) -> Result<String, BackendError>;

    /// 获取用户的所有角色名称
    async fn find_user_role_names(&self, user_id: &str) -> Result<Vec<String>, BackendError>;

    /// 获取用户的所有权限
    async fn find_user_permissions(&self, user_id: &str) -> Result<Vec<String>, BackendError>;

    /// 创建默认三角色（tenant_admin, developer, viewer）
    async fn create_default_roles(
        &self,
        tenant_id: &str,
    ) -> Result<DefaultRoles, BackendError>;
}

/// 默认三角色的 ID
#[derive(Debug, Clone)]
pub struct DefaultRoles {
    pub admin_role_id: String,
    pub developer_role_id: String,
    pub viewer_role_id: String,
}
