// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::common;
use dts_server::services::data_masking::{MaskingService, PrivacyKey};
use std::sync::{Arc, OnceLock};

pub(super) fn service(ctx: &common::TestBackend) -> Arc<MaskingService> {
    static KEY: OnceLock<PrivacyKey> = OnceLock::new();
    let key = KEY
        .get_or_init(|| {
            let directory = tempfile::tempdir().unwrap();
            PrivacyKey::load_or_create(directory.path().to_str().unwrap()).unwrap()
        })
        .clone();
    Arc::new(MaskingService::new(
        ctx.product_resource_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        ctx.user_repo.clone(),
        key,
    ))
}
