// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
use super::{BiShareFilter, CreateBiShareParams};
use crate::BackendError;
pub fn text(value: &str) -> Result<(), BackendError> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(BackendError::ConstraintViolation(
            "分享真实身份或节点标识无效".into(),
        ));
    }
    Ok(())
}
pub fn create(params: &CreateBiShareParams) -> Result<(), BackendError> {
    for value in [
        &params.id,
        &params.tenant_id,
        &params.owner_id,
        &params.recipient_id,
        &params.report_id,
        &params.job_id,
        &params.node_id,
    ] {
        text(value)?;
    }
    if params.report_version <= 0 || params.owner_id == params.recipient_id {
        return Err(BackendError::ConstraintViolation(
            "分享必须绑定有效版本和另一名真实用户".into(),
        ));
    }
    Ok(())
}
pub fn filter(params: &BiShareFilter) -> Result<(), BackendError> {
    text(&params.tenant_id)?;
    text(&params.actor_id)?;
    if let Some(id) = &params.report_id {
        text(id)?;
    }
    if !(1..=100).contains(&params.limit) {
        return Err(BackendError::ConstraintViolation(
            "分享分页必须为1至100条".into(),
        ));
    }
    Ok(())
}
