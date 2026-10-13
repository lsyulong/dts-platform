// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Mutex;

use super::*;
use crate::services::datasource_svc::DatasourceService;

#[derive(Default)]
struct MemoryRepo {
    rows: Mutex<HashMap<(String, String), CreateDatasourceParams>>,
}

impl MemoryRepo {
    fn stored_password(&self, id: &str, tenant_id: &str) -> anyhow::Result<Option<String>> {
        let rows = self
            .rows
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture lock failed"))?;
        Ok(rows
            .get(&(tenant_id.into(), id.into()))
            .and_then(|params| params.encrypted_password.clone()))
    }
}

#[async_trait]
impl DatasourceRepo for MemoryRepo {
    async fn create(&self, params: &CreateDatasourceParams) -> Result<String, BackendError> {
        self.rows
            .lock()
            .map_err(|_| BackendError::Internal("fixture lock failed".into()))?
            .insert(
                (params.tenant_id.clone(), params.id.clone()),
                params.clone(),
            );
        Ok(params.id.clone())
    }

    async fn get_connection_info(
        &self,
        id: &str,
        tenant_id: &str,
    ) -> Result<Option<RawConnectionInfo>, BackendError> {
        let rows = self
            .rows
            .lock()
            .map_err(|_| BackendError::Internal("fixture lock failed".into()))?;
        Ok(rows
            .get(&(tenant_id.into(), id.into()))
            .map(|params| RawConnectionInfo {
                db_type: params.db_type.clone(),
                host: params.host.clone(),
                port: params.port,
                username: params.username.clone(),
                password: params.encrypted_password.clone(),
                database_name: params.database_name.clone(),
            }))
    }

    async fn list_by_tenant(&self, _tenant_id: &str) -> Result<Vec<DatasourceInfo>, BackendError> {
        Ok(Vec::new())
    }
    async fn find_detail_by_id(
        &self,
        _id: &str,
        _tenant_id: &str,
    ) -> Result<Option<DatasourceDetail>, BackendError> {
        Ok(None)
    }
    async fn delete(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError> {
        Ok(self
            .rows
            .lock()
            .map_err(|_| BackendError::Internal("fixture lock failed".into()))?
            .remove(&(tenant_id.into(), id.into()))
            .is_some())
    }
    async fn count_by_tenant(&self, tenant_id: &str) -> Result<i64, BackendError> {
        Ok(self
            .rows
            .lock()
            .map_err(|_| BackendError::Internal("fixture lock failed".into()))?
            .keys()
            .filter(|(tenant, _)| tenant == tenant_id)
            .count() as i64)
    }
    async fn count_healthy_by_tenant(&self, tenant_id: &str) -> Result<i64, BackendError> {
        self.count_by_tenant(tenant_id).await
    }
    async fn exists(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError> {
        Ok(self
            .rows
            .lock()
            .map_err(|_| BackendError::Internal("fixture lock failed".into()))?
            .contains_key(&(tenant_id.into(), id.into())))
    }
}

fn fixture_params(id: &str, password: Option<&str>) -> CreateDatasourceParams {
    CreateDatasourceParams {
        id: id.into(),
        tenant_id: "tenant".into(),
        name: "database".into(),
        db_type: "mysql".into(),
        host: "localhost".into(),
        port: 3306,
        username: Some("fixture".into()),
        encrypted_password: password.map(str::to_owned),
        database_name: Some("app".into()),
        created_by: "user".into(),
    }
}

#[tokio::test]
async fn service_encrypts_at_storage_and_returns_original_password_to_engine() -> anyhow::Result<()>
{
    let memory = Arc::new(MemoryRepo::default());
    let protected: Arc<dyn DatasourceRepo> = Arc::new(SecuredDatasourceRepo {
        inner: memory.clone(),
        cipher: CredentialCipher::from_key([7; KEY_BYTES])?,
    });
    let service = DatasourceService::new(protected.clone());
    for password in [
        "synthetic-password",
        "enc:literal-prefix",
        "aesgcm:v1:literal-prefix",
        "",
    ] {
        let id = service
            .create(
                "tenant",
                "db",
                "mysql",
                "localhost",
                3306,
                Some("fixture"),
                Some(password),
                Some("app"),
                "user",
            )
            .await?;
        let stored = memory
            .stored_password(&id, "tenant")?
            .ok_or_else(|| anyhow::anyhow!("missing stored credential"))?;
        assert!(stored.starts_with(CIPHERTEXT_PREFIX));
        assert_ne!(stored, password);
        if !password.is_empty() {
            assert!(!stored.contains(password));
        }
        let connection = protected
            .get_connection_info(&id, "tenant")
            .await?
            .ok_or_else(|| anyhow::anyhow!("missing connection"))?;
        assert_eq!(connection.password.as_deref(), Some(password));
    }
    let id = service
        .create(
            "tenant",
            "db",
            "mysql",
            "localhost",
            3306,
            None,
            None,
            None,
            "user",
        )
        .await?;
    assert!(memory.stored_password(&id, "tenant")?.is_none());
    Ok(())
}

#[tokio::test]
async fn legacy_prefix_is_decoded_once_and_wrong_tenant_cannot_read() -> anyhow::Result<()> {
    let memory = Arc::new(MemoryRepo::default());
    memory
        .create(&fixture_params("legacy", Some("enc:enc:literal")))
        .await?;
    let protected = SecuredDatasourceRepo {
        inner: memory,
        cipher: CredentialCipher::from_key([7; KEY_BYTES])?,
    };
    let connection = protected
        .get_connection_info("legacy", "tenant")
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing legacy credential"))?;
    assert_eq!(connection.password.as_deref(), Some("enc:literal"));
    assert!(
        protected
            .get_connection_info("legacy", "other")
            .await?
            .is_none()
    );
    Ok(())
}

#[test]
fn cipher_rejects_tampering_wrong_key_and_changed_row_identity() -> anyhow::Result<()> {
    let cipher = CredentialCipher::from_key([7; KEY_BYTES])?;
    let encrypted = cipher.encrypt("synthetic-password", "tenant", "source")?;
    assert!(cipher.decrypt(&encrypted, "other", "source").is_err());
    assert!(cipher.decrypt(&encrypted, "tenant", "other").is_err());
    assert!(
        CredentialCipher::from_key([8; KEY_BYTES])?
            .decrypt(&encrypted, "tenant", "source")
            .is_err()
    );

    let mut bytes = STANDARD.decode(
        encrypted
            .strip_prefix(CIPHERTEXT_PREFIX)
            .ok_or_else(|| anyhow::anyhow!("missing ciphertext prefix"))?,
    )?;
    let byte = bytes
        .last_mut()
        .ok_or_else(|| anyhow::anyhow!("empty ciphertext"))?;
    *byte ^= 1;
    let tampered = format!("{CIPHERTEXT_PREFIX}{}", STANDARD.encode(bytes));
    assert!(cipher.decrypt(&tampered, "tenant", "source").is_err());
    assert!(
        cipher
            .decrypt("aesgcm:v1:AAAA", "tenant", "source")
            .is_err()
    );
    assert!(
        cipher
            .decrypt("unrecognized-plaintext", "tenant", "source")
            .is_err()
    );
    assert_ne!(
        encrypted,
        cipher.encrypt("synthetic-password", "tenant", "source")?
    );
    Ok(())
}

#[test]
fn notification_secret_uses_a_separate_authenticated_scope() -> anyhow::Result<()> {
    let cipher = CredentialCipher::from_key([7; KEY_BYTES])?;
    let encrypted = cipher.encrypt_notification("bot-secret", "tenant", "connection")?;
    assert!(encrypted.starts_with(CIPHERTEXT_PREFIX));
    assert!(!encrypted.contains("bot-secret"));
    assert_eq!(
        cipher.decrypt_notification(&encrypted, "tenant", "connection")?,
        "bot-secret"
    );
    assert!(
        cipher
            .decrypt_notification(&encrypted, "other", "connection")
            .is_err()
    );
    assert!(cipher.decrypt(&encrypted, "tenant", "connection").is_err());
    Ok(())
}

#[test]
fn persistent_key_survives_restart_and_invalid_file_is_never_replaced() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join(KEY_FILE);
    let key = load_persistent_key(&path)?;
    let cipher = CredentialCipher::from_key(key)?;
    let encrypted = cipher.encrypt("synthetic-password", "tenant", "source")?;
    let restarted = CredentialCipher::from_key(load_persistent_key(&path)?)?;
    assert_eq!(
        restarted.decrypt(&encrypted, "tenant", "source")?,
        "synthetic-password"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(path.metadata()?.permissions().mode() & 0o777, 0o600);
    }
    std::fs::write(&path, "invalid")?;
    assert!(load_persistent_key(&path).is_err());
    assert_eq!(std::fs::read_to_string(&path)?, "invalid");
    Ok(())
}

#[test]
fn encoded_key_requires_exactly_32_bytes() {
    assert!(CredentialCipher::from_encoded_key(&STANDARD.encode([7; KEY_BYTES])).is_ok());
    assert!(CredentialCipher::from_encoded_key(&STANDARD.encode([7; 31])).is_err());
    assert!(CredentialCipher::from_encoded_key("invalid base64").is_err());
    assert!(CredentialCipher::from_encoded_key("").is_err());
}
