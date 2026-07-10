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

        tracing::info!("租户 '{name}' 创建成功, 管理员: {admin_username}");

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use dts_backend::repo::role::DefaultRoles;
    use dts_backend::BackendError;

    // ============================================================
    // Mock TenantRepo
    // ============================================================

    struct MockTenantRepo {
        existing_slug: Mutex<Option<String>>,
        created_tenants: Mutex<Vec<CreateTenantParams>>,
        list_result: Mutex<Vec<TenantInfo>>,
        find_by_id_result: Mutex<Option<TenantInfo>>,
    }

    impl MockTenantRepo {
        fn new() -> Self {
            Self {
                existing_slug: Mutex::new(None),
                created_tenants: Mutex::new(vec![]),
                list_result: Mutex::new(vec![]),
                find_by_id_result: Mutex::new(None),
            }
        }

        fn set_existing_slug(&self, slug: Option<&str>) {
            *self.existing_slug.lock().unwrap() = slug.map(|s| s.to_string());
        }

        fn set_list_result(&self, tenants: Vec<TenantInfo>) {
            *self.list_result.lock().unwrap() = tenants;
        }

        fn set_find_by_id_result(&self, tenant: Option<TenantInfo>) {
            *self.find_by_id_result.lock().unwrap() = tenant;
        }

        fn created_tenants(&self) -> Vec<CreateTenantParams> {
            self.created_tenants.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl TenantRepo for MockTenantRepo {
        async fn create(&self, params: &CreateTenantParams) -> Result<String, BackendError> {
            self.created_tenants.lock().unwrap().push(params.clone());
            Ok(params.id.clone())
        }

        async fn find_by_slug(
            &self,
            slug: &str,
        ) -> Result<Option<TenantInfo>, BackendError> {
            let existing = self.existing_slug.lock().unwrap();
            if let Some(ref s) = *existing {
                if s == slug {
                    return Ok(Some(TenantInfo {
                        id: "existing-id".into(),
                        name: "Existing".into(),
                        slug: s.clone(),
                        status: "active".into(),
                        created_at: chrono::Utc::now(),
                    }));
                }
            }
            Ok(None)
        }

        async fn find_by_id(
            &self,
            _id: &str,
        ) -> Result<Option<TenantInfo>, BackendError> {
            Ok(self.find_by_id_result.lock().unwrap().clone())
        }

        async fn list_all(&self) -> Result<Vec<TenantInfo>, BackendError> {
            Ok(self.list_result.lock().unwrap().clone())
        }

        async fn count(&self) -> Result<i64, BackendError> {
            Ok(self.list_result.lock().unwrap().len() as i64)
        }
    }

    // ============================================================
    // Mock RoleRepo
    // ============================================================

    struct MockRoleRepo {
        created_roles_tenant_ids: Mutex<Vec<String>>,
        role_names: Mutex<Vec<String>>,
        permissions: Mutex<Vec<String>>,
    }

    impl MockRoleRepo {
        fn new() -> Self {
            Self {
                created_roles_tenant_ids: Mutex::new(vec![]),
                role_names: Mutex::new(vec![]),
                permissions: Mutex::new(vec![]),
            }
        }

        fn created_roles_tenant_ids(&self) -> Vec<String> {
            self.created_roles_tenant_ids.lock().unwrap().clone()
        }

        #[allow(dead_code)]
        fn set_role_names(&self, names: Vec<String>) {
            *self.role_names.lock().unwrap() = names;
        }

        #[allow(dead_code)]
        fn set_permissions(&self, perms: Vec<String>) {
            *self.permissions.lock().unwrap() = perms;
        }
    }

    #[async_trait]
    impl RoleRepo for MockRoleRepo {
        async fn create_with_permissions(
            &self,
            _params: &CreateRoleParams,
        ) -> Result<String, BackendError> {
            Ok("role-id".into())
        }

        async fn find_user_role_names(
            &self,
            _user_id: &str,
        ) -> Result<Vec<String>, BackendError> {
            Ok(self.role_names.lock().unwrap().clone())
        }

        async fn find_user_permissions(
            &self,
            _user_id: &str,
        ) -> Result<Vec<String>, BackendError> {
            Ok(self.permissions.lock().unwrap().clone())
        }

        async fn create_default_roles(
            &self,
            tenant_id: &str,
        ) -> Result<DefaultRoles, BackendError> {
            self.created_roles_tenant_ids
                .lock()
                .unwrap()
                .push(tenant_id.to_string());
            Ok(DefaultRoles {
                admin_role_id: "admin-role-id".into(),
                developer_role_id: "developer-role-id".into(),
                viewer_role_id: "viewer-role-id".into(),
            })
        }
    }

    // ============================================================
    // Mock UserRepo (minimal for TenantService tests)
    // ============================================================

    struct MockUserRepo {
        created_users: Mutex<Vec<CreateUserParams>>,
        role_assignments: Mutex<Vec<(String, String)>>,
        find_result: Mutex<Option<dts_backend::types::UserRecord>>,
    }

    impl MockUserRepo {
        fn new() -> Self {
            Self {
                created_users: Mutex::new(vec![]),
                role_assignments: Mutex::new(vec![]),
                find_result: Mutex::new(None),
            }
        }

        #[allow(dead_code)]
        fn set_find_result(&self, record: Option<dts_backend::types::UserRecord>) {
            *self.find_result.lock().unwrap() = record;
        }

        fn created_users(&self) -> Vec<CreateUserParams> {
            self.created_users.lock().unwrap().clone()
        }

        fn role_assignments(&self) -> Vec<(String, String)> {
            self.role_assignments.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl UserRepo for MockUserRepo {
        async fn create(&self, params: &CreateUserParams) -> Result<String, BackendError> {
            self.created_users.lock().unwrap().push(params.clone());
            Ok(params.id.clone())
        }

        async fn find_by_tenant_and_username(
            &self,
            _tenant_id: &str,
            _username: &str,
        ) -> Result<Option<dts_backend::types::UserRecord>, BackendError> {
            Ok(self.find_result.lock().unwrap().clone())
        }

        async fn find_detail_by_id(
            &self,
            _user_id: &str,
        ) -> Result<Option<dts_backend::repo::user::UserDetail>, BackendError> {
            Ok(None)
        }

        async fn assign_role(&self, user_id: &str, role_id: &str) -> Result<(), BackendError> {
            self.role_assignments
                .lock()
                .unwrap()
                .push((user_id.to_string(), role_id.to_string()));
            Ok(())
        }
    }

    // ============================================================
    // create_tenant Tests
    // ============================================================

    #[tokio::test]
    async fn create_tenant_succeeds_with_all_data() {
        let tenant_repo = Arc::new(MockTenantRepo::new());
        let role_repo = Arc::new(MockRoleRepo::new());
        let user_repo = Arc::new(MockUserRepo::new());

        let svc = TenantService::new(tenant_repo.clone(), role_repo.clone(), user_repo.clone());
        let result = svc
            .create_tenant("TestCo", "testco", "admin_testco", "admin@testco.com", "hashed_pw")
            .await;

        assert!(result.is_ok());
        let tenant_id = result.unwrap();
        assert_eq!(tenant_id.len(), 36, "Tenant ID should be a UUID");

        let created = tenant_repo.created_tenants();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].name, "TestCo");
        assert_eq!(created[0].slug, "testco");

        let role_tenant_ids = role_repo.created_roles_tenant_ids();
        assert_eq!(role_tenant_ids.len(), 1);
        assert_eq!(role_tenant_ids[0], tenant_id);

        let users = user_repo.created_users();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].username, "admin_testco");
        assert_eq!(users[0].email, "admin@testco.com");
        assert_eq!(users[0].password_hash, "hashed_pw");
        assert_eq!(users[0].display_name.as_deref(), Some("TestCo管理员"));

        let assignments = user_repo.role_assignments();
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].1, "admin-role-id");
    }

    #[tokio::test]
    async fn create_tenant_rejects_duplicate_slug() {
        let tenant_repo = Arc::new(MockTenantRepo::new());
        let role_repo = Arc::new(MockRoleRepo::new());
        let user_repo = Arc::new(MockUserRepo::new());

        tenant_repo.set_existing_slug(Some("duplicate"));

        let svc = TenantService::new(tenant_repo.clone(), role_repo.clone(), user_repo.clone());
        let result = svc
            .create_tenant("Dup", "duplicate", "admin", "a@d.com", "pw")
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("已存在"), "Expected 'already exists' error, got: {err}");
    }

    #[tokio::test]
    async fn create_tenant_produces_unique_ids() {
        let tenant_repo = Arc::new(MockTenantRepo::new());
        let role_repo = Arc::new(MockRoleRepo::new());
        let user_repo = Arc::new(MockUserRepo::new());

        let svc = TenantService::new(tenant_repo.clone(), role_repo.clone(), user_repo.clone());

        let id1 = svc
            .create_tenant("A", "a", "admin_a", "a@a.com", "pw")
            .await
            .unwrap();
        let id2 = svc
            .create_tenant("B", "b", "admin_b", "b@b.com", "pw")
            .await
            .unwrap();

        assert_ne!(id1, id2, "Each tenant should have a unique UUID");

        let created = tenant_repo.created_tenants();
        assert_eq!(created.len(), 2);
        assert_ne!(created[0].id, created[1].id);

        let users = user_repo.created_users();
        assert_eq!(users.len(), 2);
        assert_ne!(users[0].id, users[1].id);
    }

    // ============================================================
    // list Tests
    // ============================================================

    #[tokio::test]
    async fn list_delegates_to_repo() {
        let tenant_repo = Arc::new(MockTenantRepo::new());
        let role_repo = Arc::new(MockRoleRepo::new());
        let user_repo = Arc::new(MockUserRepo::new());

        tenant_repo.set_list_result(vec![
            TenantInfo {
                id: "t1".into(),
                name: "T1".into(),
                slug: "t1".into(),
                status: "active".into(),
                created_at: chrono::Utc::now(),
            },
            TenantInfo {
                id: "t2".into(),
                name: "T2".into(),
                slug: "t2".into(),
                status: "active".into(),
                created_at: chrono::Utc::now(),
            },
        ]);

        let svc = TenantService::new(tenant_repo.clone(), role_repo.clone(), user_repo.clone());
        let result = svc.list().await;

        assert!(result.is_ok());
        let tenants = result.unwrap();
        assert_eq!(tenants.len(), 2);
        assert_eq!(tenants[0].slug, "t1");
        assert_eq!(tenants[1].slug, "t2");
    }

    #[tokio::test]
    async fn list_returns_empty_when_no_tenants() {
        let tenant_repo = Arc::new(MockTenantRepo::new());
        let role_repo = Arc::new(MockRoleRepo::new());
        let user_repo = Arc::new(MockUserRepo::new());

        let svc = TenantService::new(tenant_repo.clone(), role_repo.clone(), user_repo.clone());
        let result = svc.list().await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // ============================================================
    // get_by_id Tests
    // ============================================================

    #[tokio::test]
    async fn get_by_id_returns_tenant_when_found() {
        let tenant_repo = Arc::new(MockTenantRepo::new());
        let role_repo = Arc::new(MockRoleRepo::new());
        let user_repo = Arc::new(MockUserRepo::new());

        tenant_repo.set_find_by_id_result(Some(TenantInfo {
            id: "t-found".into(),
            name: "Found".into(),
            slug: "found".into(),
            status: "active".into(),
            created_at: chrono::Utc::now(),
        }));

        let svc = TenantService::new(tenant_repo.clone(), role_repo.clone(), user_repo.clone());
        let result = svc.get_by_id("t-found").await;

        assert!(result.is_ok());
        let tenant = result.unwrap();
        assert_eq!(tenant.id, "t-found");
        assert_eq!(tenant.name, "Found");
    }

    #[tokio::test]
    async fn get_by_id_returns_error_when_not_found() {
        let tenant_repo = Arc::new(MockTenantRepo::new());
        let role_repo = Arc::new(MockRoleRepo::new());
        let user_repo = Arc::new(MockUserRepo::new());

        let svc = TenantService::new(tenant_repo.clone(), role_repo.clone(), user_repo.clone());
        let result = svc.get_by_id("nonexistent").await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("不存在"), "Expected 'not found' error, got: {err}");
    }
}
