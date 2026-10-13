use super::{SqliteDataAccessRepo, audit, authority, grants, receipts, requests, storage};
use dts_backend::{BackendError, repo::data_access::*};

pub(super) async fn decide(
    repo: &SqliteDataAccessRepo,
    params: &DecideDataAccessParams,
) -> Result<DataAccessDecisionResult, BackendError> {
    let semantics = validation::decision_request(params)?;
    let mut tx = repo.begin(&params.tenant_id, &params.actor_id).await?;
    let authority = authority::get(&mut tx, &params.tenant_id, &params.actor_id, true).await?;
    if params.decision.needs_approver() && !authority.can_approve {
        return Err(storage::invalid("缺少 data_security:approve 权限"));
    }
    if let Some(result) = receipts::replay(&mut tx, params, &semantics).await? {
        tx.commit().await.map_err(storage::error)?;
        return Ok(result);
    }
    let request = requests::get(&mut tx, &params.tenant_id, &params.request_id)
        .await?
        .ok_or_else(|| BackendError::NotFound("数据访问申请不存在".into()))?;
    if request.resource.version != params.expected_version {
        return Err(storage::conflict("申请版本已经改变"));
    }
    let status = next_status(&request, params)?;
    if params.decision == DataAccessDecision::Submit
        && (request.expires_at <= storage::now(&mut tx).await?
            || !storage::verified(&mut tx, &params.tenant_id, &request.target).await?)
    {
        return Err(storage::conflict("申请期限或元数据版本已经失效"));
    }
    let version = request
        .resource
        .version
        .checked_add(1)
        .ok_or_else(|| storage::conflict("申请版本耗尽"))?;
    let grant = match params.decision {
        DataAccessDecision::Approve => {
            Some(grants::create(&mut tx, &request, params, version).await?)
        },
        DataAccessDecision::Revoke => Some(grants::revoke(&mut tx, params).await?),
        DataAccessDecision::Withdraw if request.resource.status == "approved" => {
            Some(grants::revoke(&mut tx, params).await?)
        },
        _ => None,
    };
    let mut changed = request.resource.clone();
    changed.status = status.into();
    changed.version = version;
    changed.updated_at = storage::now(&mut tx).await?;
    let saved =
        storage::write(&mut tx, &params.actor_id, Some(&request.resource), &changed).await?;
    let result = DataAccessDecisionResult {
        request: saved,
        grant,
        decision_id: uuid::Uuid::new_v4().to_string(),
        replayed: false,
    };
    receipts::save(&mut tx, params, &semantics, &result).await?;
    audit::decision(&mut tx, params, &result).await?;
    tx.commit().await.map_err(storage::error)?;
    Ok(result)
}

fn next_status(
    request: &DataAccessRequest,
    params: &DecideDataAccessParams,
) -> Result<&'static str, BackendError> {
    let owner = request.requester_id == params.actor_id;
    if params.decision.needs_approver() && owner {
        return Err(storage::invalid("不能审批或撤销本人的申请，请使用本人撤回"));
    }
    match (params.decision, request.resource.status.as_str()) {
        (DataAccessDecision::Submit, "draft" | "rejected") if owner => Ok("pending"),
        (DataAccessDecision::Approve, "pending") => Ok("approved"),
        (DataAccessDecision::Reject, "pending") => Ok("rejected"),
        (DataAccessDecision::Withdraw, "draft" | "pending" | "approved") if owner => {
            Ok("withdrawn")
        },
        (DataAccessDecision::Revoke, "approved") => Ok("revoked"),
        _ => Err(storage::conflict("申请状态不允许当前操作或并非本人申请")),
    }
}
