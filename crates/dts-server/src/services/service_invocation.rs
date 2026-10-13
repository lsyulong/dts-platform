use crate::{
    models::error::AppError,
    services::{
        data_access::{AccessOutcome, AuthorizedQuery, QueryAuthorizationService},
        data_masking::{MaskingPlan, MaskingService},
        data_services::{DataApiService, PreparedDataApiCall},
        product_jobs::ReadOnlyQueryExecutor,
        query_svc::QueryResult,
        service_access::{CallPermit, ServiceAccessService},
    },
};
use dts_backend::repo::service_access::ServiceCallOutcome;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceInvokeRequest {
    #[serde(default = "empty_parameters")]
    pub parameters: Value,
}
fn empty_parameters() -> Value {
    json!({})
}

/// External callers can only invoke the approved frozen API through an opaque
/// permit. They cannot supply a tenant, execution identity, SQL or masking plan.
#[derive(Clone)]
pub struct ServiceInvocationService {
    access: ServiceAccessService,
    apis: DataApiService,
    queries: Arc<dyn ReadOnlyQueryExecutor>,
    authorization: Arc<QueryAuthorizationService>,
    masking: Arc<MaskingService>,
}
struct InvocationGuard {
    prepared: PreparedDataApiCall,
    query: AuthorizedQuery,
    plan: MaskingPlan,
}
impl ServiceInvocationService {
    pub fn new(
        access: ServiceAccessService,
        apis: DataApiService,
        queries: Arc<dyn ReadOnlyQueryExecutor>,
        authorization: Arc<QueryAuthorizationService>,
        masking: Arc<MaskingService>,
    ) -> Self {
        Self {
            access,
            apis,
            queries,
            authorization,
            masking,
        }
    }

    pub async fn invoke(
        &self,
        secret: &str,
        api_id: &str,
        origin: Option<&str>,
        request: ServiceInvokeRequest,
    ) -> Result<Value, AppError> {
        let permit = self.access.authenticate(secret, api_id, origin).await?;
        match self.invoke_reserved(&permit, request.parameters).await {
            Ok(result) => Ok(result),
            Err(error) => {
                // A missing final receipt remains pending; never claim success
                // if the actual terminal audit could not be committed.
                self.access
                    .finish(&permit, failure_outcome(&error), None)
                    .await?;
                Err(error)
            },
        }
    }

    async fn prepare(
        &self,
        permit: &CallPermit,
        parameters: &Value,
    ) -> Result<InvocationGuard, AppError> {
        let prepared = self
            .apis
            .prepare_invocation(
                permit.actor(),
                permit.api_id(),
                permit.api_version(),
                parameters,
            )
            .await?;
        let query = self
            .authorization
            .authorize(permit.actor(), prepared.sql())
            .await?;
        let plan = match self.masking.prepare_output(permit.actor(), &query).await {
            Ok(plan) => plan,
            Err(error) => {
                self.authorization
                    .record_access(permit.actor(), &query, AccessOutcome::Denied)
                    .await?;
                return Err(error);
            },
        };
        Ok(InvocationGuard {
            prepared,
            query,
            plan,
        })
    }

    async fn reauthorize(
        &self,
        permit: &CallPermit,
        guard: &InvocationGuard,
    ) -> Result<(), AppError> {
        self.access.reauthorize(permit).await?;
        self.apis
            .reauthorize_invocation(permit.actor(), &guard.prepared)
            .await?;
        self.authorization
            .reauthorize(permit.actor(), &guard.query)
            .await?;
        self.masking
            .reauthorize_plan(permit.actor(), &guard.plan)
            .await
    }

    async fn guarded_query(
        &self,
        permit: &CallPermit,
        guard: &InvocationGuard,
    ) -> Result<QueryResult, AppError> {
        self.reauthorize(permit, guard).await?;
        let execution = self.queries.execute(guard.query.sql());
        tokio::pin!(execution);
        let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        heartbeat.tick().await;
        tokio::time::timeout(Duration::from_secs(65), async {
            loop {
                tokio::select! {
                    result = &mut execution => return result,
                    _ = heartbeat.tick() => self.reauthorize(permit, guard).await?,
                }
            }
        })
        .await
        .map_err(|_| AppError::BadRequest("数据 API 查询超过执行时限".into()))?
    }

    async fn invoke_reserved(
        &self,
        permit: &CallPermit,
        parameters: Value,
    ) -> Result<Value, AppError> {
        let guard = self.prepare(permit, &parameters).await?;
        let result = self.protected_result(permit, &guard).await;
        let mut result = match result {
            Ok(result) => result,
            Err(error) => {
                self.authorization
                    .record_access(permit.actor(), &guard.query, access_outcome(&error))
                    .await?;
                return Err(error);
            },
        };
        result["invocation_scope"] = json!("approved_application_credentials");
        result["rate_limit_enforced"] = json!(true);
        // Budget the complete response before committing a terminal receipt.
        // u64::MAX reserves all possible digits of the actual measured duration.
        result["call"] = json!({
            "id": permit.call_id(), "application_id": permit.application_id(),
            "api_id": permit.api_id(), "api_version": permit.api_version(),
            "outcome": "succeeded", "duration_ms": u64::MAX,
            "external_credential_authenticated": true, "rate_limit_enforced": true,
        });
        if let Err(error) = response_bound(&result) {
            self.authorization
                .record_access(permit.actor(), &guard.query, AccessOutcome::Failed)
                .await?;
            return Err(error);
        }
        // Recheck after encoding and immediately before the success receipt.
        // finish_with_access() verifies grant and privacy fences in the same
        // transaction as the actual access audit and call terminal receipt.
        if let Err(error) = self.reauthorize(permit, &guard).await {
            self.authorization
                .record_access(permit.actor(), &guard.query, AccessOutcome::Denied)
                .await?;
            return Err(error);
        }
        let row_count = result["row_count"]
            .as_u64()
            .ok_or_else(|| AppError::Internal("数据 API 结果缺少实际行数".into()))?;
        let finished = match self
            .access
            .finish_with_access(permit, &guard.query, &guard.plan, row_count)
            .await
        {
            Ok(finished) => finished,
            Err(error) => {
                self.authorization
                    .record_access(permit.actor(), &guard.query, AccessOutcome::Denied)
                    .await?;
                return Err(error);
            },
        };
        result["call"]["duration_ms"] = json!(finished.duration_ms);
        Ok(result)
    }

    async fn protected_result(
        &self,
        permit: &CallPermit,
        guard: &InvocationGuard,
    ) -> Result<Value, AppError> {
        let raw = self.guarded_query(permit, guard).await?;
        let protected =
            self.apis
                .protect_invocation(&guard.prepared, raw, &self.masking, &guard.plan)?;
        self.reauthorize(permit, guard).await?;
        self.apis.map_invocation(&guard.prepared, protected)
    }
}

fn response_bound(result: &Value) -> Result<(), AppError> {
    let bytes = serde_json::to_vec(result)
        .map_err(|_| AppError::Internal("数据 API 结果无法编码".into()))?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(AppError::BadRequest(
            "数据 API 完整响应超过 2 MiB，请减少行数或字段".into(),
        ));
    }
    Ok(())
}

fn failure_outcome(error: &AppError) -> ServiceCallOutcome {
    if matches!(
        error,
        AppError::Unauthorized(_)
            | AppError::Forbidden(_)
            | AppError::Conflict(_)
            | AppError::NotFound(_)
    ) {
        ServiceCallOutcome::Denied
    } else {
        ServiceCallOutcome::Failed
    }
}
fn access_outcome(error: &AppError) -> AccessOutcome {
    if failure_outcome(error) == ServiceCallOutcome::Denied {
        AccessOutcome::Denied
    } else {
        AccessOutcome::Failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caller_cannot_supply_identity_sql_or_publication_version() {
        for field in [
            "tenant_id",
            "actor_id",
            "sql",
            "expected_version",
            "masking_plan",
        ] {
            assert!(
                serde_json::from_value::<ServiceInvokeRequest>(json!({(field): "spoof"})).is_err()
            );
        }
        let request: ServiceInvokeRequest =
            serde_json::from_value(json!({"parameters":{"area":"华东"}})).expect("typed request");
        assert_eq!(request.parameters["area"], "华东");
    }
    #[test]
    fn revocations_are_denials_and_engine_errors_are_failures() {
        assert_eq!(
            failure_outcome(&AppError::Conflict("撤回".into())),
            ServiceCallOutcome::Denied
        );
        assert_eq!(
            failure_outcome(&AppError::Internal("引擎".into())),
            ServiceCallOutcome::Failed
        );
    }
    #[test]
    fn byte_limit_includes_the_actual_call_metadata() {
        let mut response = json!({"rows":"x".repeat(2 * 1024 * 1024 - 32)});
        assert!(response_bound(&response).is_ok());
        response["call"] = json!({"id":"actual-call-id", "duration_ms":u64::MAX});
        assert!(response_bound(&response).is_err());
    }
}
