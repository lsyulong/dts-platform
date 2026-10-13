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

use std::sync::Arc;

use async_trait::async_trait;
use dts_backend::repo::RoleRepo;
use sa_token_adapter::storage::SaStorage;
use sa_token_core::config::TokenStyle;
use sa_token_core::{
    ExactMatcher, GrantWritePolicy, SaTokenConfig, SaTokenError, SaTokenManager, SaTokenResult,
    StpInterface,
};
use sa_token_storage_memory::MemoryStorage;

use crate::config::AuthConfig;

pub const LOGIN_TYPE: &str = "dts-user";

struct DatabaseGrants {
    roles: Arc<dyn RoleRepo>,
}

#[async_trait]
impl StpInterface for DatabaseGrants {
    async fn get_permission_list(
        &self,
        login_id: &str,
        login_type: &str,
    ) -> SaTokenResult<Vec<String>> {
        if login_type != LOGIN_TYPE {
            return Ok(Vec::new());
        }
        self.roles
            .find_user_permissions(login_id)
            .await
            .map_err(|_| SaTokenError::StorageError("权限数据读取失败".into()))
    }

    async fn get_role_list(&self, login_id: &str, login_type: &str) -> SaTokenResult<Vec<String>> {
        if login_type != LOGIN_TYPE {
            return Ok(Vec::new());
        }
        self.roles
            .find_user_role_names(login_id)
            .await
            .map_err(|_| SaTokenError::StorageError("角色数据读取失败".into()))
    }
}

fn configured_manager(
    storage: Arc<dyn SaStorage>,
    roles: Arc<dyn RoleRepo>,
    timeout: i64,
) -> Arc<SaTokenManager> {
    let config = SaTokenConfig {
        token_name: "Authorization".into(),
        token_style: TokenStyle::Random64,
        timeout,
        active_timeout: -1,
        auto_renew: false,
        is_concurrent: true,
        is_share: false,
        is_log: false,
        is_read_cookie: false,
        is_read_header: true,
        is_read_body: false,
        storage_key_prefix: "dts:auth:".into(),
        grant_cache_ttl: 0,
        grant_request_scope: false,
        grant_write_policy: GrantWritePolicy::Reject,
        ..SaTokenConfig::default()
    };
    Arc::new(
        SaTokenManager::new(storage, config)
            .with_stp_interface(Arc::new(DatabaseGrants { roles }))
            .with_permission_matcher(Arc::new(ExactMatcher)),
    )
}

/// Construct once before the Actix worker factory; every worker shares this instance.
pub async fn build_manager(
    config: &AuthConfig,
    roles: Arc<dyn RoleRepo>,
) -> anyhow::Result<Arc<SaTokenManager>> {
    config.validate()?;
    let storage: Arc<dyn SaStorage> = match config.storage_backend.as_str() {
        "memory" => memory_storage(),
        "redis" => redis_storage(config).await?,
        _ => anyhow::bail!("AUTH_STORAGE 必须为 memory 或 redis"),
    };
    Ok(configured_manager(
        storage,
        roles,
        config.token_timeout_secs,
    ))
}

fn memory_storage() -> Arc<dyn SaStorage> {
    let storage = Arc::new(MemoryStorage::new());
    let weak = Arc::downgrade(&storage);
    // Collect expired sessions without keeping the owning service alive.
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Some(storage) = weak.upgrade() else {
                break;
            };
            storage.cleanup_expired().await;
        }
    });
    storage
}

#[cfg(feature = "auth-redis")]
async fn redis_storage(config: &AuthConfig) -> anyhow::Result<Arc<dyn SaStorage>> {
    let url = config
        .redis_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Redis 会话存储需要 AUTH_REDIS_URL"))?;
    // Core already prefixes logical keys; avoid adding a second Redis prefix.
    let storage = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        sa_token_storage_redis::RedisStorage::new(url, ""),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Redis 会话存储连接超时"))?
    .map_err(|_| anyhow::anyhow!("Redis 会话存储连接失败"))?;
    Ok(Arc::new(storage))
}

#[cfg(not(feature = "auth-redis"))]
async fn redis_storage(_: &AuthConfig) -> anyhow::Result<Arc<dyn SaStorage>> {
    anyhow::bail!("Redis 会话存储需要编译 auth-redis feature")
}

/// Isolated framework instance for integration fixtures, with the production grant policy.
pub fn test_manager(roles: Arc<dyn RoleRepo>) -> Arc<SaTokenManager> {
    configured_manager(Arc::new(MemoryStorage::new()), roles, 86_400)
}
