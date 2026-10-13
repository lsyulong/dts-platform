use super::{IssuedCredential, ServiceAccessService, bad, denied};
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::{data_masking::PrivacyKey, product::ResourceAction},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use dts_backend::repo::service_access::*;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;

#[derive(Clone)]
pub struct CredentialKey(PrivacyKey);
impl CredentialKey {
    pub fn load_or_create(data_dir: &str) -> anyhow::Result<Self> {
        Ok(Self(PrivacyKey::load_named(
            data_dir,
            "service-credential-hmac.key",
        )?))
    }
    fn input(tenant: &str, id: &str, secret: &[u8]) -> Vec<u8> {
        let mut data = b"dts.service.credential.v1".to_vec();
        for part in [tenant.as_bytes(), id.as_bytes(), secret] {
            data.extend_from_slice(&(part.len() as u64).to_be_bytes());
            data.extend_from_slice(part);
        }
        data
    }
    pub(super) fn verifier(&self, tenant: &str, id: &str, secret: &[u8]) -> String {
        self.0.sign_hex(&Self::input(tenant, id, secret))
    }
    pub(super) fn verify(&self, tenant: &str, id: &str, secret: &[u8], verifier: &str) -> bool {
        self.0
            .verify_hex(&Self::input(tenant, id, secret), verifier)
    }
    #[cfg(test)]
    pub(super) fn fixture(bytes: [u8; 32]) -> Self {
        Self(PrivacyKey::from_test_bytes(bytes))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IssuePayload {
    idempotency_key: String,
    expires_at: Option<DateTime<Utc>>,
}
impl ServiceAccessService {
    pub async fn issue_credential(
        &self,
        actor: &AuthIdentity,
        id: &str,
        action: ResourceAction,
    ) -> Result<IssuedCredential, AppError> {
        let payload: IssuePayload = serde_json::from_value(action.payload)
            .map_err(|_| bad("凭据签发只接受idempotency_key/可选expires_at"))?;
        self.authority(actor).await?;
        let application = self
            .repo
            .get_application(&actor.tenant_id, &actor.sub, id)
            .await?
            .ok_or_else(|| AppError::NotFound("服务应用不存在或不可管理".into()))?;
        let expires = payload
            .expires_at
            .unwrap_or(application.credential_expires_at);
        let credential_id = uuid::Uuid::new_v4().to_string();
        let mut secret = [0u8; 32];
        SystemRandom::new()
            .fill(&mut secret)
            .map_err(|_| AppError::Internal("服务凭据安全随机数生成失败".into()))?;
        let verifier = self.key.verifier(&actor.tenant_id, &credential_id, &secret);
        let result = self
            .repo
            .issue_credential(&IssueServiceCredentialParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                application_id: id.into(),
                expected_version: action.expected_version,
                idempotency_key: payload.idempotency_key,
                credential_id: credential_id.clone(),
                verifier,
                requested_expires_at: payload.expires_at,
                expires_at: expires,
            })
            .await?;
        let token = if result.replayed {
            None
        } else {
            if result.credential.id != credential_id {
                return Err(AppError::Internal("凭据仓储返回与本次签发身份不符".into()));
            }
            Some(format!(
                "dts_app_{credential_id}.{}",
                URL_SAFE_NO_PAD.encode(secret)
            ))
        };
        Ok(IssuedCredential {
            application: result.application,
            credential: result.credential,
            issued: true,
            secret_not_retrievable: result.replayed,
            secret: token,
            replayed: result.replayed,
        })
    }
    pub async fn revoke_credential(
        &self,
        actor: &AuthIdentity,
        application_id: &str,
        credential_id: &str,
        expected_version: i64,
        key: &str,
    ) -> Result<ServiceCredentialSummary, AppError> {
        self.authority(actor).await?;
        Ok(self
            .repo
            .revoke_credential(&RevokeServiceCredentialParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                application_id: application_id.into(),
                credential_id: credential_id.into(),
                expected_version,
                idempotency_key: key.into(),
            })
            .await?)
    }
    pub async fn list_credentials(
        &self,
        actor: &AuthIdentity,
        application_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<ServiceCredentialPage, AppError> {
        self.authority(actor).await?;
        validation::pagination(limit, offset)?;
        Ok(self
            .repo
            .list_credentials(&ServiceCredentialFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                application_id: application_id.into(),
                limit,
                offset,
            })
            .await?)
    }
}

pub(super) fn parse_token(value: &str) -> Result<(String, Vec<u8>), AppError> {
    if value.len() > 256 {
        return Err(denied("应用凭据无效"));
    }
    let (id, encoded) = value
        .strip_prefix("dts_app_")
        .and_then(|value| value.split_once('.'))
        .ok_or_else(|| denied("应用凭据无效"))?;
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| denied("应用凭据无效"))?;
    if parsed.to_string() != id {
        return Err(denied("应用凭据无效"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .ok()
        .filter(|bytes| bytes.len() == 32)
        .ok_or_else(|| denied("应用凭据无效"))?;
    if URL_SAFE_NO_PAD.encode(&bytes) != encoded {
        return Err(denied("应用凭据无效"));
    }
    Ok((id.into(), bytes))
}
