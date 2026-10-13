use super::{MetadataCatalogRepo, QualityService, QualitySnapshot};
use crate::models::error::AppError;
use dts_backend::repo::metadata_catalog::binding_matches;
use std::sync::Arc;

impl QualityService {
    pub fn with_metadata_repo(mut self, metadata: Arc<dyn MetadataCatalogRepo>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub(super) async fn verified_metadata(
        &self,
        snapshot: &QualitySnapshot,
    ) -> Result<bool, AppError> {
        let Some(repo) = self.metadata.as_ref() else {
            return Ok(false);
        };
        let Some(binding) = repo
            .verified_binding(
                &snapshot.tenant_id,
                &snapshot.metadata.id,
                snapshot.metadata.version,
            )
            .await?
        else {
            return Ok(false);
        };
        Ok(binding.engine == "swanlake"
            && binding.tenant_id == snapshot.tenant_id
            && binding.object_version == snapshot.metadata.version
            && binding_matches(&binding, &snapshot.metadata.properties))
    }
}
