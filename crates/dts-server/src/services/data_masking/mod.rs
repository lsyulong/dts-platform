mod configuration;
mod key;
mod output;
mod policy;
mod types;

pub use key::PrivacyKey;
pub use types::{MaskingOutputDescriptor, MaskingPlan, MaskingReceipt};

use crate::models::error::AppError;
use dts_backend::{ProductResourceRepo, UserRepo, repo::metadata_catalog::MetadataCatalogRepo};
use std::sync::Arc;

#[derive(Clone)]
pub struct MaskingService {
    resources: Arc<dyn ProductResourceRepo>,
    metadata: Arc<dyn MetadataCatalogRepo>,
    users: Arc<dyn UserRepo>,
    key: PrivacyKey,
}

impl MaskingService {
    pub fn new(
        resources: Arc<dyn ProductResourceRepo>,
        metadata: Arc<dyn MetadataCatalogRepo>,
        users: Arc<dyn UserRepo>,
        key: PrivacyKey,
    ) -> Self {
        Self {
            resources,
            metadata,
            users,
            key,
        }
    }
}

fn bad(message: impl Into<String>) -> AppError {
    AppError::BadRequest(message.into())
}
fn denied(message: impl Into<String>) -> AppError {
    AppError::Forbidden(message.into())
}

#[cfg(test)]
mod tests;
