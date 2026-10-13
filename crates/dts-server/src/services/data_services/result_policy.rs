// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
use super::{
    bad, compiler, result,
    types::{ApiPublication, ProtectedDataApiResult},
};
use crate::{
    models::error::AppError,
    services::{
        data_masking::{MaskingPlan, MaskingService},
        query_svc::QueryResult,
    },
};
use dts_backend::repo::product_job::ProductJob;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn validate_native_node_result(
    job: &ProductJob,
    value: &QueryResult,
) -> Result<(), AppError> {
    let publication = compiler::snapshot(job)?.publication;
    result::validate_result(
        &publication.api.response_schema,
        publication.api.row_limit,
        value,
    )
}
pub(crate) fn protect_node_result(
    job: &ProductJob,
    raw: QueryResult,
    masking: &MaskingService,
    plan: &MaskingPlan,
) -> Result<ProtectedDataApiResult, AppError> {
    let sql = compiler::prepare_claimed(job)?
        .into_iter()
        .next()
        .ok_or_else(|| bad("API真实执行节点缺失"))?
        .1;
    if !plan.is_bound_to(&job.tenant_id, &job.actor_id, &sql) {
        return Err(AppError::Forbidden(
            "API脱敏计划不属于当前任务执行身份或SQL".into(),
        ));
    }
    protect(&compiler::snapshot(job)?.publication, raw, masking, plan)
}
pub(crate) fn map_protected_node_result(
    job: &ProductJob,
    value: ProtectedDataApiResult,
) -> Result<Value, AppError> {
    let mut value = map_protected_result(&compiler::snapshot(job)?.publication, value)?;
    value["invocation_scope"] = json!("authenticated_console_jobs");
    value["external_credentials_ready"] = json!(false);
    value["rate_limit_enforced"] = json!(false);
    Ok(value)
}
pub(crate) fn map_stored_node_result(
    job: &ProductJob,
    masked: QueryResult,
    plan: &MaskingPlan,
) -> Result<Value, AppError> {
    let publication = compiler::snapshot(job)?.publication;
    map_protected_node_result(job, protected(&publication, masked, plan)?)
}
pub(super) fn protect(
    publication: &ApiPublication,
    raw: QueryResult,
    masking: &MaskingService,
    plan: &MaskingPlan,
) -> Result<ProtectedDataApiResult, AppError> {
    // No raw JSON response is built. Masking cannot conceal physical schema
    // drift, invalid source values or an overflow sentinel.
    result::validate_result(
        &publication.api.response_schema,
        publication.api.row_limit,
        &raw,
    )?;
    protected(publication, masking.apply_output(plan, raw)?, plan)
}
fn protected(
    publication: &ApiPublication,
    value: QueryResult,
    plan: &MaskingPlan,
) -> Result<ProtectedDataApiResult, AppError> {
    let mut schema = publication.api.response_schema.clone();
    let mut seen = BTreeSet::new();
    for output in plan.output_columns() {
        if !seen.insert(output.name.to_ascii_lowercase()) || output.data_type != "VARCHAR" {
            return Err(bad("API输出计划字段重复或类型不受支持"));
        }
        let field = schema
            .iter_mut()
            .find(|field| field.name.eq_ignore_ascii_case(&output.name))
            .ok_or_else(|| bad("脱敏计划输出字段不属于已发布API投影"))?;
        field.data_type = output.data_type;
        field.nullable |= output.may_introduce_null;
    }
    result::validate_result(&schema, publication.api.row_limit, &value)?;
    Ok(ProtectedDataApiResult {
        result: value,
        source_id: publication.source_id.clone(),
        source_version: publication.source_version,
        schema,
        policy_hash: plan.policy_hash().into(),
        mask_applied: plan.has_masks(),
        invocation_identity: None,
    })
}
pub(super) fn map_protected_result(
    publication: &ApiPublication,
    value: ProtectedDataApiResult,
) -> Result<Value, AppError> {
    if value.source_id != publication.source_id
        || value.source_version != publication.source_version
    {
        return Err(bad("API受保护结果不属于当前发布版本"));
    }
    result::validate_result(&value.schema, publication.api.row_limit, &value.result)?;
    let mut mapped = result::map_value(publication, value.result);
    mapped["source_response_schema"] = json!(publication.api.response_schema);
    mapped["response_schema"] = json!(value.schema);
    mapped["effective_response_schema"] = json!(value.schema);
    mapped["schema_policy_hash"] = json!(value.policy_hash);
    mapped["mask_applied"] = json!(value.mask_applied);
    Ok(mapped)
}
