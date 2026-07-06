use std::sync::Arc;

use anyhow::anyhow;
use dts_backend::repo::{RoleRepo, TenantRepo, UserRepo};
use dts_backend::types::*;
use uuid::Uuid;

/// 租户管理服务
#[derive(Clone)]
pub struct TenantService {
    pub tenant_repo: Arc<dyn TenantRepo>,
    pub role_repo: Arc<dyn RoleRepo>,
    pub user_repo: Arc<dyn UserRepo>,
}

impl TenantService {
    pub fn new(
        tenant_repo: Arc<dyn TenantRepo>,
        role_repo: Arc<dyn RoleRepo>,
        user_repo: Arc<dyn UserRepo>,
    ) -> Self {
        Self {
            tenant_repo,
            role_repo,
            user_repo,
        }
    }

    /// 创建租户（同时创建默认角色和 admin 用户）
    pub async fn create_tenant(
        &self,
        name: &str,
        slug: &str,
        admin_username: &str,
        admin_email: &str,
        admin_password_hash: &str,
    ) -> anyhow::Result<String> {
        // 检查 slug 唯一性
        let existing = self.tenant_repo.find_by_slug(slug).await?;
        if existing.is_some() {
            return Err(anyhow!("租户标识 '{slug}' 已存在"));
        }

        let tenant_id = Uuid::new_v4().to_string();

        // 创建租户
        self.tenant_repo
            .create(&CreateTenantParams {
                id: tenant_id.clone(),
                name: name.into(),
                slug: slug.into(),
            })
            .await?;

        // 创建默认角色
        let roles = self.role_repo.create_default_roles(&tenant_id).await?;

        // 创建管理员用户
        let user_id = Uuid::new_v4().to_string();
        self.user_repo
            .create(&CreateUserParams {
                id: user_id.clone(),
                tenant_id: tenant_id.clone(),
                username: admin_username.into(),
                email: admin_email.into(),
                password_hash: admin_password_hash.into(),
                display_name: Some(format!("{name}管理员")),
            })
            .await?;

        // 分配 admin 角色
        self.user_repo.assign_role(&user_id, &roles.admin_role_id).await?;

        log::info!("租户 '{name}' 创建成功, 管理员: {admin_username}");

        Ok(tenant_id)
    }

    /// 获取租户列表
    pub async fn list(&self) -> anyhow::Result<Vec<TenantInfo>> {
        Ok(self.tenant_repo.list_all().await?)
    }

    /// 获取单个租户
    pub async fn get_by_id(&self, id: &str) -> anyhow::Result<TenantInfo> {
        self.tenant_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| anyhow!("租户不存在"))
    }
}

// 测试需要 PostgreSQL 环境，移至集成测试
