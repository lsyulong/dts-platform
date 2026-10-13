use crate::models::auth::AuthIdentity;
use dts_backend::{
    ProductResource,
    repo::service_access::{ServiceCallRecord, ServiceCredentialSummary},
};
use serde::Serialize;
use std::time::Instant;

#[derive(Serialize)]
pub struct IssuedCredential {
    pub application: ProductResource,
    pub credential: ServiceCredentialSummary,
    pub issued: bool,
    pub secret: Option<String>,
    pub secret_not_retrievable: bool,
    pub replayed: bool,
}
impl std::fmt::Debug for IssuedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedCredential")
            .field("credential", &self.credential)
            .field("secret", &"<REDACTED>")
            .field("replayed", &self.replayed)
            .finish()
    }
}

/// Authenticated and atomically reserved by the service only. HTTP payloads
/// cannot deserialize, create or alter a permit or its effective owner.
#[derive(Clone)]
pub struct CallPermit {
    pub(super) record: ServiceCallRecord,
    pub(super) actor: AuthIdentity,
    pub(super) started: Instant,
}
impl CallPermit {
    pub fn actor(&self) -> &AuthIdentity {
        &self.actor
    }
    pub fn api_id(&self) -> &str {
        &self.record.api_id
    }
    pub fn api_version(&self) -> i64 {
        self.record.api_version
    }
    pub fn call_id(&self) -> &str {
        &self.record.id
    }
    pub fn application_id(&self) -> &str {
        &self.record.application_id
    }
}
impl std::fmt::Debug for CallPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallPermit")
            .field("call_id", &self.record.id)
            .field("tenant_id", &self.record.tenant_id)
            .field("api_id", &self.record.api_id)
            .field("api_version", &self.record.api_version)
            .finish()
    }
}
