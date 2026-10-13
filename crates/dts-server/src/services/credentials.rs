// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use dts_backend::BackendError;
use dts_backend::repo::{
    CreateNotificationConnectionParams, DatasourceProbe, DatasourceRepo, NotificationConnection,
    NotificationConnectionRepo, RawConnectionInfo, RecordDatasourceProbeParams,
    RecordNotificationConnectionTestParams, SetNotificationConnectionEnabledParams,
    UpdateNotificationConnectionParams,
};
use dts_backend::types::{CreateDatasourceParams, DatasourceDetail, DatasourceInfo};
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};

const CIPHERTEXT_PREFIX: &str = "aesgcm:v1:";
const KEY_FILE: &str = ".credentials-key";
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;

/// Wrap every consumer of the datasource repository once at application setup.
/// Incoming create passwords are plaintext; storage receives authenticated
/// ciphertext. Only the internal connection-info operation decrypts credentials.
pub fn protect_repo(
    inner: Arc<dyn DatasourceRepo>,
    data_dir: &str,
) -> anyhow::Result<Arc<dyn DatasourceRepo>> {
    let cipher = match std::env::var("DTS_CREDENTIAL_KEY") {
        Ok(encoded) => CredentialCipher::from_encoded_key(&encoded)?,
        Err(std::env::VarError::NotPresent) => CredentialCipher::from_key(load_persistent_key(
            &PathBuf::from(data_dir).join(KEY_FILE),
        )?)?,
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("DTS_CREDENTIAL_KEY 必须是 32 字节密钥的 Base64 编码")
        },
    };
    Ok(Arc::new(SecuredDatasourceRepo { inner, cipher }))
}

/// Encrypt notification provider secrets before persistence and decrypt them only
/// for internal delivery or connection tests.
pub fn protect_notification_repo(
    inner: Arc<dyn NotificationConnectionRepo>,
    data_dir: &str,
) -> anyhow::Result<Arc<dyn NotificationConnectionRepo>> {
    let cipher = load_cipher(data_dir)?;
    Ok(Arc::new(SecuredNotificationConnectionRepo {
        inner,
        cipher,
    }))
}

fn load_cipher(data_dir: &str) -> anyhow::Result<CredentialCipher> {
    match std::env::var("DTS_CREDENTIAL_KEY") {
        Ok(encoded) => CredentialCipher::from_encoded_key(&encoded),
        Err(std::env::VarError::NotPresent) => CredentialCipher::from_key(load_persistent_key(
            &PathBuf::from(data_dir).join(KEY_FILE),
        )?),
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("DTS_CREDENTIAL_KEY 必须是 32 字节密钥的 Base64 编码")
        },
    }
}

struct CredentialCipher {
    key: LessSafeKey,
}

impl CredentialCipher {
    fn from_encoded_key(encoded: &str) -> anyhow::Result<Self> {
        Self::from_key(decode_key(encoded)?)
    }

    fn from_key(bytes: [u8; KEY_BYTES]) -> anyhow::Result<Self> {
        let key = UnboundKey::new(&AES_256_GCM, &bytes)
            .map_err(|_| anyhow::anyhow!("数据源凭据密钥初始化失败"))?;
        Ok(Self {
            key: LessSafeKey::new(key),
        })
    }

    fn encrypt(
        &self,
        value: &str,
        tenant_id: &str,
        datasource_id: &str,
    ) -> Result<String, BackendError> {
        let mut nonce = [0; NONCE_BYTES];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| BackendError::Internal("数据源凭据随机数生成失败".into()))?;
        let mut payload = value.as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(credential_aad(tenant_id, datasource_id)),
                &mut payload,
            )
            .map_err(|_| BackendError::Internal("数据源凭据加密失败".into()))?;
        let mut envelope = Vec::with_capacity(NONCE_BYTES + payload.len());
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&payload);
        Ok(format!("{CIPHERTEXT_PREFIX}{}", STANDARD.encode(envelope)))
    }

    fn decrypt(
        &self,
        encoded: &str,
        tenant_id: &str,
        datasource_id: &str,
    ) -> Result<String, BackendError> {
        if let Some(legacy) = encoded.strip_prefix("enc:") {
            tracing::warn!("读取旧版未加密数据源凭据，请重新创建该数据源以升级凭据存储");
            return Ok(legacy.to_owned());
        }
        let payload = encoded
            .strip_prefix(CIPHERTEXT_PREFIX)
            .ok_or_else(credential_error)?;
        let mut envelope = STANDARD.decode(payload).map_err(|_| credential_error())?;
        if envelope.len() < NONCE_BYTES + TAG_BYTES {
            return Err(credential_error());
        }
        let nonce = Nonce::try_assume_unique_for_key(&envelope[..NONCE_BYTES])
            .map_err(|_| credential_error())?;
        let plaintext = self
            .key
            .open_in_place(
                nonce,
                Aad::from(credential_aad(tenant_id, datasource_id)),
                &mut envelope[NONCE_BYTES..],
            )
            .map_err(|_| credential_error())?;
        String::from_utf8(plaintext.to_vec()).map_err(|_| credential_error())
    }

    fn encrypt_notification(
        &self,
        value: &str,
        tenant: &str,
        id: &str,
    ) -> Result<String, BackendError> {
        self.encrypt_scoped(value, b"dts.notification.secret.v1", tenant, id)
    }

    fn decrypt_notification(
        &self,
        value: &str,
        tenant: &str,
        id: &str,
    ) -> Result<String, BackendError> {
        self.decrypt_scoped(value, b"dts.notification.secret.v1", tenant, id)
    }

    fn encrypt_scoped(
        &self,
        value: &str,
        namespace: &[u8],
        tenant: &str,
        id: &str,
    ) -> Result<String, BackendError> {
        let mut nonce = [0; NONCE_BYTES];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| BackendError::Internal("凭据随机数生成失败".into()))?;
        let mut payload = value.as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(scoped_aad(namespace, tenant, id)),
                &mut payload,
            )
            .map_err(|_| BackendError::Internal("凭据加密失败".into()))?;
        let mut envelope = Vec::with_capacity(NONCE_BYTES + payload.len());
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&payload);
        Ok(format!("{CIPHERTEXT_PREFIX}{}", STANDARD.encode(envelope)))
    }

    fn decrypt_scoped(
        &self,
        encoded: &str,
        namespace: &[u8],
        tenant: &str,
        id: &str,
    ) -> Result<String, BackendError> {
        let payload = encoded
            .strip_prefix(CIPHERTEXT_PREFIX)
            .ok_or_else(notification_credential_error)?;
        let mut envelope = STANDARD
            .decode(payload)
            .map_err(|_| notification_credential_error())?;
        if envelope.len() < NONCE_BYTES + TAG_BYTES {
            return Err(notification_credential_error());
        }
        let nonce = Nonce::try_assume_unique_for_key(&envelope[..NONCE_BYTES])
            .map_err(|_| notification_credential_error())?;
        let plain = self
            .key
            .open_in_place(
                nonce,
                Aad::from(scoped_aad(namespace, tenant, id)),
                &mut envelope[NONCE_BYTES..],
            )
            .map_err(|_| notification_credential_error())?;
        String::from_utf8(plain.to_vec()).map_err(|_| notification_credential_error())
    }
}

fn credential_error() -> BackendError {
    BackendError::Configuration("数据源凭据校验失败，请检查密钥和凭据完整性".into())
}

fn credential_aad(tenant_id: &str, datasource_id: &str) -> Vec<u8> {
    let mut aad = b"dts.datasource.password.v1".to_vec();
    for part in [tenant_id, datasource_id] {
        aad.extend_from_slice(&(part.len() as u64).to_be_bytes());
        aad.extend_from_slice(part.as_bytes());
    }
    aad
}

fn scoped_aad(namespace: &[u8], tenant: &str, id: &str) -> Vec<u8> {
    let mut aad = namespace.to_vec();
    for part in [tenant, id] {
        aad.extend_from_slice(&(part.len() as u64).to_be_bytes());
        aad.extend_from_slice(part.as_bytes());
    }
    aad
}

fn notification_credential_error() -> BackendError {
    BackendError::Configuration("通知连接凭据校验失败，请检查密钥和凭据完整性".into())
}

fn decode_key(encoded: &str) -> anyhow::Result<[u8; KEY_BYTES]> {
    let decoded = STANDARD
        .decode(encoded.trim())
        .map_err(|_| anyhow::anyhow!("凭据密钥必须是 32 字节密钥的 Base64 编码"))?;
    decoded
        .try_into()
        .map_err(|_| anyhow::anyhow!("凭据密钥长度必须是 32 字节"))
}

fn read_key(file: File) -> anyhow::Result<[u8; KEY_BYTES]> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            anyhow::bail!("凭据密钥文件权限过宽，请设置为 0600");
        }
    }
    let mut encoded = String::new();
    file.take(129).read_to_string(&mut encoded)?;
    if encoded.len() > 128 {
        anyhow::bail!("凭据密钥文件格式无效");
    }
    decode_key(&encoded)
}

fn load_persistent_key(path: &Path) -> anyhow::Result<[u8; KEY_BYTES]> {
    match File::open(path) {
        Ok(file) => return read_key(file),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
        Err(error) => return Err(anyhow::anyhow!("无法读取凭据密钥文件: {error}")),
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("凭据密钥目录无效"))?;
    std::fs::create_dir_all(parent)?;
    let mut key = [0; KEY_BYTES];
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|_| anyhow::anyhow!("凭据密钥随机数生成失败"))?;

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(STANDARD.encode(key).as_bytes())?;
            file.sync_all()?;
            Ok(key)
        },
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // A racing process owns creation. Never replace its key; an incomplete
            // or invalid file fails closed rather than silently changing the key.
            read_key(File::open(path)?)
        },
        Err(error) => Err(anyhow::anyhow!("无法创建凭据密钥文件: {error}")),
    }
}

struct SecuredDatasourceRepo {
    inner: Arc<dyn DatasourceRepo>,
    cipher: CredentialCipher,
}

#[async_trait]
impl DatasourceRepo for SecuredDatasourceRepo {
    async fn create(&self, params: &CreateDatasourceParams) -> Result<String, BackendError> {
        let mut secured = params.clone();
        secured.encrypted_password = params
            .encrypted_password
            .as_deref()
            .map(|password| self.cipher.encrypt(password, &params.tenant_id, &params.id))
            .transpose()?;
        self.inner.create(&secured).await
    }

    async fn get_connection_info(
        &self,
        id: &str,
        tenant_id: &str,
    ) -> Result<Option<RawConnectionInfo>, BackendError> {
        let Some(mut connection) = self.inner.get_connection_info(id, tenant_id).await? else {
            return Ok(None);
        };
        connection.password = connection
            .password
            .as_deref()
            .map(|password| self.cipher.decrypt(password, tenant_id, id))
            .transpose()?;
        Ok(Some(connection))
    }

    async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<DatasourceInfo>, BackendError> {
        self.inner.list_by_tenant(tenant_id).await
    }

    async fn find_detail_by_id(
        &self,
        id: &str,
        tenant_id: &str,
    ) -> Result<Option<DatasourceDetail>, BackendError> {
        self.inner.find_detail_by_id(id, tenant_id).await
    }

    async fn delete(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError> {
        self.inner.delete(id, tenant_id).await
    }

    async fn count_by_tenant(&self, tenant_id: &str) -> Result<i64, BackendError> {
        self.inner.count_by_tenant(tenant_id).await
    }

    async fn count_healthy_by_tenant(&self, tenant_id: &str) -> Result<i64, BackendError> {
        self.inner.count_healthy_by_tenant(tenant_id).await
    }

    async fn exists(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError> {
        self.inner.exists(id, tenant_id).await
    }

    async fn record_probe(
        &self,
        params: &RecordDatasourceProbeParams,
    ) -> Result<DatasourceProbe, BackendError> {
        self.inner.record_probe(params).await
    }

    async fn list_probes(
        &self,
        tenant_id: &str,
        datasource_id: &str,
        limit: u32,
    ) -> Result<Vec<DatasourceProbe>, BackendError> {
        self.inner
            .list_probes(tenant_id, datasource_id, limit)
            .await
    }
}

struct SecuredNotificationConnectionRepo {
    inner: Arc<dyn NotificationConnectionRepo>,
    cipher: CredentialCipher,
}

#[async_trait]
impl NotificationConnectionRepo for SecuredNotificationConnectionRepo {
    async fn create(
        &self,
        p: &CreateNotificationConnectionParams,
    ) -> Result<NotificationConnection, BackendError> {
        let mut secured = p.clone();
        secured.secret = p
            .secret
            .as_deref()
            .map(|value| self.cipher.encrypt_notification(value, &p.tenant_id, &p.id))
            .transpose()?;
        self.inner.create(&secured).await.map(|mut value| {
            value.secret = None;
            value
        })
    }
    async fn list(&self, tenant: &str) -> Result<Vec<NotificationConnection>, BackendError> {
        self.inner.list(tenant).await
    }
    async fn get(
        &self,
        tenant: &str,
        id: &str,
    ) -> Result<Option<NotificationConnection>, BackendError> {
        let Some(mut value) = self.inner.get(tenant, id).await? else {
            return Ok(None);
        };
        value.secret = value
            .secret
            .as_deref()
            .map(|secret| self.cipher.decrypt_notification(secret, tenant, id))
            .transpose()?;
        Ok(Some(value))
    }
    async fn update(
        &self,
        p: &UpdateNotificationConnectionParams,
    ) -> Result<NotificationConnection, BackendError> {
        let mut secured = p.clone();
        if p.replace_secret {
            secured.secret = p
                .secret
                .as_deref()
                .map(|value| self.cipher.encrypt_notification(value, &p.tenant_id, &p.id))
                .transpose()?;
        }
        self.inner.update(&secured).await.map(|mut value| {
            value.secret = None;
            value
        })
    }
    async fn set_enabled(
        &self,
        p: &SetNotificationConnectionEnabledParams,
    ) -> Result<NotificationConnection, BackendError> {
        self.inner.set_enabled(p).await.map(|mut value| {
            value.secret = None;
            value
        })
    }
    async fn delete(
        &self,
        tenant: &str,
        id: &str,
        version: i64,
        actor: &str,
    ) -> Result<bool, BackendError> {
        self.inner.delete(tenant, id, version, actor).await
    }
    async fn record_test(
        &self,
        p: &RecordNotificationConnectionTestParams,
    ) -> Result<(), BackendError> {
        self.inner.record_test(p).await
    }
}

#[cfg(test)]
mod tests;
