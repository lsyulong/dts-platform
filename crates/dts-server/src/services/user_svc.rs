use std::sync::Arc;

use anyhow::anyhow;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use dts_backend::repo::UserRepo;
use dts_backend::types::*;
use uuid::Uuid;

/// 用户管理服务
#[derive(Clone)]
pub struct UserService {
    pub user_repo: Arc<dyn UserRepo>,
}

impl UserService {
    pub fn new(user_repo: Arc<dyn UserRepo>) -> Self {
        Self { user_repo }
    }

    /// 创建用户 — 对明文密码进行 argon2 哈希后存储
    pub async fn create_user(
        &self,
        tenant_id: &str,
        username: &str,
        email: &str,
        password: &str,
        display_name: Option<&str>,
    ) -> anyhow::Result<UserRecord> {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("密码哈希失败: {e}"))?
            .to_string();

        let user_id = Uuid::new_v4().to_string();

        let params = CreateUserParams {
            id: user_id,
            tenant_id: tenant_id.to_string(),
            username: username.to_string(),
            email: email.to_string(),
            password_hash,
            display_name: display_name.map(|s| s.to_string()),
        };

        self.user_repo.create(&params).await?;

        let user = self
            .user_repo
            .find_by_tenant_and_username(tenant_id, username)
            .await?
            .ok_or_else(|| anyhow!("用户创建后未找到"))?;

        tracing::info!("用户 '{username}' 创建成功");

        Ok(user)
    }

    /// 根据租户 ID 和用户名查找用户
    pub async fn find_by_username(
        &self,
        tenant_id: &str,
        username: &str,
    ) -> anyhow::Result<Option<UserRecord>> {
        Ok(self
            .user_repo
            .find_by_tenant_and_username(tenant_id, username)
            .await?)
    }

    /// 列出用户（支持筛选）
    pub async fn list_users(
        &self,
        tenant_id: Option<&str>,
        search: Option<&str>,
        status: Option<&str>,
    ) -> anyhow::Result<Vec<UserListInfo>> {
        Ok(self.user_repo.list_all(tenant_id, search, status).await?)
    }

    /// 更新用户基本信息（不含角色）
    pub async fn update_user(
        &self,
        user_id: &str,
        display_name: Option<String>,
        status: Option<String>,
    ) -> anyhow::Result<()> {
        self.user_repo
            .update(
                user_id,
                &UpdateUserParams {
                    display_name,
                    status,
                },
            )
            .await?;
        Ok(())
    }

    /// 更新用户角色
    pub async fn update_user_roles(
        &self,
        user_id: &str,
        role_ids: &[String],
    ) -> anyhow::Result<()> {
        self.user_repo.remove_roles(user_id).await?;
        for role_id in role_ids {
            self.user_repo.assign_role(user_id, role_id).await?;
        }
        Ok(())
    }

    /// 软删除用户
    pub async fn soft_delete_user(&self, user_id: &str) -> anyhow::Result<()> {
        Ok(self.user_repo.soft_delete(user_id).await?)
    }

    /// 重置用户密码
    pub async fn reset_user_password(
        &self,
        user_id: &str,
        new_password: &str,
    ) -> anyhow::Result<()> {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(new_password.as_bytes(), &salt)
            .map_err(|e| anyhow!("密码哈希失败: {e}"))?
            .to_string();
        Ok(self
            .user_repo
            .reset_password(user_id, &password_hash)
            .await?)
    }

    /// 哈希密码（同步，用于外部调用）
    pub fn hash_password_sync(password: &str) -> anyhow::Result<String> {
        use argon2::password_hash::PasswordHasher;
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("密码哈希失败: {e}"))?
            .to_string();
        Ok(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use dts_backend::BackendError;
    use dts_backend::repo::user::UserDetail;

    /// 手动 mock UserRepo — 捕获 create 调用参数，返回预设结果
    struct MockUserRepo {
        last_create_params: Mutex<Option<CreateUserParams>>,
        find_result: Mutex<Option<UserRecord>>,
    }

    impl MockUserRepo {
        fn new() -> Self {
            Self {
                last_create_params: Mutex::new(None),
                find_result: Mutex::new(None),
            }
        }

        fn set_find_result(&self, record: Option<UserRecord>) {
            *self.find_result.lock().unwrap() = record;
        }

        fn last_create_params(&self) -> Option<CreateUserParams> {
            self.last_create_params.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl UserRepo for MockUserRepo {
        async fn create(&self, params: &CreateUserParams) -> Result<String, BackendError> {
            *self.last_create_params.lock().unwrap() = Some(params.clone());
            Ok(params.id.clone())
        }

        async fn find_by_tenant_and_username(
            &self,
            _tenant_id: &str,
            _username: &str,
        ) -> Result<Option<UserRecord>, BackendError> {
            Ok(self.find_result.lock().unwrap().clone())
        }

        async fn find_detail_by_id(
            &self,
            _user_id: &str,
        ) -> Result<Option<UserDetail>, BackendError> {
            Ok(None)
        }

        async fn assign_role(&self, _user_id: &str, _role_id: &str) -> Result<(), BackendError> {
            Ok(())
        }

        async fn list_all(
            &self,
            _tenant_id: Option<&str>,
            _search: Option<&str>,
            _status: Option<&str>,
        ) -> Result<Vec<UserListInfo>, BackendError> {
            Ok(vec![])
        }

        async fn update(
            &self,
            _user_id: &str,
            _params: &UpdateUserParams,
        ) -> Result<(), BackendError> {
            Ok(())
        }

        async fn soft_delete(&self, _user_id: &str) -> Result<(), BackendError> {
            Ok(())
        }

        async fn reset_password(
            &self,
            _user_id: &str,
            _password_hash: &str,
        ) -> Result<(), BackendError> {
            Ok(())
        }

        async fn remove_roles(&self, _user_id: &str) -> Result<(), BackendError> {
            Ok(())
        }
    }

    fn make_user_record(id: &str, tenant_id: &str, username: &str) -> UserRecord {
        UserRecord {
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            username: username.to_string(),
            email: format!("{username}@example.com"),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$...".to_string(),
            display_name: Some(username.to_string()),
            status: "active".to_string(),
        }
    }

    /// 验证密码在存储前已被哈希（password_hash 不等于原始明文）
    #[tokio::test]
    async fn create_user_hashes_password_before_storage() {
        let plain_password = "mypassword";
        let repo = Arc::new(MockUserRepo::new());

        repo.set_find_result(Some(make_user_record("ignored", "tenant-1", "testuser")));

        let svc = UserService::new(repo.clone());
        let result = svc
            .create_user(
                "tenant-1",
                "testuser",
                "testuser@example.com",
                plain_password,
                Some("Test User"),
            )
            .await;

        assert!(result.is_ok());

        // 关键断言：存储的 password_hash 不是明文，且以 argon2 前缀开头
        let params = repo
            .last_create_params()
            .expect("create should have been called");
        assert_ne!(
            params.password_hash, plain_password,
            "password_hash should NOT be the plain text password"
        );
        assert!(
            params.password_hash.starts_with("$argon2"),
            "password_hash should start with '$argon2', got: {}",
            params.password_hash
        );
    }

    /// 验证创建的用户记录正确返回
    #[tokio::test]
    async fn create_user_returns_correct_record() {
        let repo = Arc::new(MockUserRepo::new());

        let expected = make_user_record("expected-id", "tenant-2", "jdoe");
        repo.set_find_result(Some(expected.clone()));

        let svc = UserService::new(repo.clone());
        let result = svc
            .create_user("tenant-2", "jdoe", "jdoe@example.com", "secret", None)
            .await;

        assert!(result.is_ok());
        let user = result.unwrap();
        assert_eq!(user.id, expected.id);
        assert_eq!(user.username, expected.username);
        assert_eq!(user.email, expected.email);
        assert_eq!(user.tenant_id, expected.tenant_id);
    }

    /// 验证 find_by_username 正确委托给 repo
    #[tokio::test]
    async fn find_by_username_delegates_to_repo() {
        let repo = Arc::new(MockUserRepo::new());

        repo.set_find_result(Some(make_user_record("u1", "tenant-1", "alice")));

        let svc = UserService::new(repo.clone());
        let result = svc.find_by_username("tenant-1", "alice").await;

        assert!(result.is_ok());
        let user = result.unwrap();
        assert!(user.is_some());
        assert_eq!(user.unwrap().username, "alice");
    }

    /// 验证 find_by_username 返回 None（用户不存在）
    #[tokio::test]
    async fn find_by_username_returns_none_when_not_found() {
        let repo = Arc::new(MockUserRepo::new());
        // find_result 默认为 None

        let svc = UserService::new(repo.clone());
        let result = svc.find_by_username("tenant-1", "nobody").await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    /// 验证 create_user 使用 UUID 生成唯一用户 ID
    #[tokio::test]
    async fn create_user_generates_unique_id() {
        let repo = Arc::new(MockUserRepo::new());

        repo.set_find_result(Some(make_user_record("x", "tenant-1", "testuser")));

        let svc = UserService::new(repo.clone());
        let _ = svc
            .create_user("tenant-1", "testuser", "e@e.com", "pw", None)
            .await;

        let params = repo
            .last_create_params()
            .expect("create should have been called");
        assert_eq!(params.id.len(), 36, "UUID v4 must be 36 characters");
        assert_eq!(
            params.id.chars().filter(|c| *c == '-').count(),
            4,
            "UUID v4 must have 4 dashes"
        );
    }
}
