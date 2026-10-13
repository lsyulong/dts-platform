// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;

use dts_backend::repo::{
    metadata_catalog::{MetadataApplyResult, MetadataEngineBinding, binding_matches},
    product_job::*,
};
use serde::{Serialize, Serializer, ser::Error};
use serde_json::{Value, json};

use super::{ProductJobService, runner::compile_claimed, stored_result};
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::data_access::AccessOutcome,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultAccess {
    Authorized,
    Restricted,
    Summary,
    NotAvailable,
}

#[derive(Debug, Clone, Serialize)]
struct NodeAccess {
    access: ResultAccess,
    reason: Option<&'static str>,
}

/// A reader-specific projection. Its private job already has sensitive fields
/// removed; the durable execution record is never changed by this projection.
#[derive(Debug, Clone)]
pub struct ProductJobView {
    job: ProductJob,
    pub result_access: ResultAccess,
    pub result_access_reason: Option<&'static str>,
    node_access: Vec<NodeAccess>,
}

impl Deref for ProductJobView {
    type Target = ProductJob;
    fn deref(&self) -> &Self::Target {
        &self.job
    }
}

impl Serialize for ProductJobView {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serde_json::to_value(&self.job).map_err(S::Error::custom)?;
        value["result_access"] = json!(self.result_access);
        value["result_access_reason"] = json!(self.result_access_reason);
        if let Some(nodes) = value["nodes"].as_array_mut() {
            for (node, access) in nodes.iter_mut().zip(&self.node_access) {
                node["result_access"] = json!(access.access);
                node["result_access_reason"] = json!(access.reason);
            }
        }
        value.serialize(serializer)
    }
}

#[derive(Debug, Serialize)]
pub struct ProductJobPageView {
    pub data: Vec<ProductJobView>,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct EnqueuedProductJobView {
    pub job: ProductJobView,
    pub created: bool,
}

impl ProductJobService {
    pub async fn submission_view(
        &self,
        reader: &AuthIdentity,
        submitted: EnqueuedProductJob,
    ) -> Result<EnqueuedProductJobView, AppError> {
        Ok(EnqueuedProductJobView {
            job: self.get(reader, &submitted.job.id).await?,
            created: submitted.created,
        })
    }

    pub(super) async fn result_view(
        &self,
        reader: &AuthIdentity,
        mut job: ProductJob,
    ) -> ProductJobView {
        let original = job.clone();
        let queries = compile_claimed(&original);
        let terminal = matches!(
            job.status,
            ProductJobStatus::Succeeded
                | ProductJobStatus::Failed
                | ProductJobStatus::Cancelled
                | ProductJobStatus::ReconciliationRequired
        );
        let mut accesses = Vec::with_capacity(job.nodes.len());
        for node in &mut job.nodes {
            let protected = if node.result.is_none() {
                Ok(None)
            } else if !terminal {
                Err("job_in_progress")
            } else if node.status != ProductJobNodeStatus::Succeeded {
                Err("result_not_successful")
            } else if let Ok(queries) = &queries {
                if let Some((_, sql)) = queries.iter().find(|(id, _)| id == &node.id) {
                    self.authorize_saved_result(
                        reader,
                        &original,
                        &node.id,
                        sql,
                        node.result.as_ref(),
                    )
                    .await
                    .map(Some)
                } else {
                    Err("definition_invalid")
                }
            } else {
                Err("definition_invalid")
            };
            let (access, reason) = match protected {
                Ok(Some(value)) => {
                    node.result = Some(value);
                    (ResultAccess::Authorized, None)
                },
                Ok(None) => (ResultAccess::NotAvailable, None),
                Err(reason) => {
                    node.result = None;
                    (ResultAccess::Restricted, Some(reason))
                },
            };
            accesses.push(NodeAccess { access, reason });
        }
        let restricted = accesses
            .iter()
            .find(|access| access.access == ResultAccess::Restricted);
        let result_access = if let Some(access) = restricted {
            job.result = None;
            (ResultAccess::Restricted, access.reason)
        } else if original.result.is_some() {
            // Top-level results are executor summaries only. Never release an
            // arbitrary stored value independently of its authorized nodes.
            job.result = control_summary(&original);
            if job.result.is_some() {
                (ResultAccess::Authorized, None)
            } else {
                (ResultAccess::Restricted, Some("result_invalid"))
            }
        } else if accesses
            .iter()
            .any(|a| a.access == ResultAccess::Authorized)
        {
            (ResultAccess::Authorized, None)
        } else {
            (ResultAccess::NotAvailable, None)
        };
        sanitize_definition(&mut job);
        ProductJobView {
            job,
            result_access: result_access.0,
            result_access_reason: result_access.1,
            node_access: accesses,
        }
    }

    async fn authorize_saved_result(
        &self,
        reader: &AuthIdentity,
        job: &ProductJob,
        node_id: &str,
        sql: &str,
        stored: Option<&Value>,
    ) -> Result<Value, &'static str> {
        self.verify_runtime(reader, job, false)
            .await
            .map_err(|_| "runtime_configuration_invalidated")?;
        if job.source_kind == "metadata_collection" {
            self.authorize_read(reader, "metadata_collection")
                .await
                .map_err(|_| "authorization_denied")?;
            return metadata_summary(stored.cloned()).ok_or("result_invalid");
        }
        self.verify_saved_domain(reader, job).await?;
        let service = self
            .query_authorization
            .as_ref()
            .ok_or("authorization_unconfigured")?;
        let query = if job.source_kind == "quality_task" {
            service
                .authorize_quality_result(reader, job, node_id, sql)
                .await
        } else if job.source_kind == "recognition_task" {
            service
                .authorize_recognition_result(reader, job, node_id, sql)
                .await
        } else if crate::services::metrics::is_calculable(&job.source_kind) {
            service
                .authorize_metric_result(reader, job, node_id, sql)
                .await
        } else {
            service.authorize(reader, sql).await
        }
        .map_err(|_| "authorization_denied")?;
        let masking = self.masking.as_ref().ok_or("privacy_unconfigured")?;
        let plan = match masking.prepare_output(reader, &query).await {
            Ok(plan) => plan,
            Err(_) => {
                service
                    .record_access(reader, &query, AccessOutcome::Denied)
                    .await
                    .map_err(|_| "audit_unavailable")?;
                return Err("privacy_policy_invalid");
            },
        };
        let stored = stored.ok_or("result_invalid")?;
        let protected = if job.source_kind == "recognition_task" {
            serde_json::from_value::<dts_backend::repo::sensitive_findings::SensitiveScanResult>(
                stored.clone(),
            )
            .map_err(|_| "result_invalid")
            .and_then(|result| {
                if !result.producer_committed
                    || result.definition_hash
                        != dts_backend::repo::sensitive_findings::snapshot_hash(
                            &job.definition_snapshot,
                        )
                        .map_err(|_| "result_invalid")?
                {
                    return Err("result_invalid");
                }
                serde_json::to_value(result).map_err(|_| "result_invalid")
            })
        } else if job.source_kind == "quality_task" {
            // The opaque quality authorization reconstructs the fixed counter
            // query. A typed counter result cannot contain raw row samples.
            serde_json::from_value::<crate::services::data_quality::QualityCheckResult>(
                stored.clone(),
            )
            .and_then(serde_json::to_value)
            .map_err(|_| "result_invalid")
        } else {
            let decoded = stored_result::decode(stored).map_err(|_| "result_invalid")?;
            if job.source_kind == "data_api" && decoded.1.is_none() {
                // Legacy unsigned data must satisfy the frozen source schema
                // before a current output plan can transform it.
                crate::services::data_services::validate_native_node_result(job, &decoded.0)
                    .map_err(|_| "result_invalid")?;
            }
            match masking
                .apply_stored_output(reader, &query, decoded.0, decoded.1.as_ref())
                .await
            {
                Ok(result) => masking
                    .seal_output(&plan, &result)
                    .map_err(|_| "privacy_policy_invalid")
                    .and_then(|receipt| {
                        stored_result::project(job, result, receipt, &plan)
                            .map_err(|_| "result_invalid")
                    }),
                Err(_) => Err("privacy_policy_changed"),
            }
        };
        let protected = match protected {
            Ok(protected) => protected,
            Err(reason) => {
                service
                    .record_access(reader, &query, AccessOutcome::Denied)
                    .await
                    .map_err(|_| "audit_unavailable")?;
                return Err(reason);
            },
        };
        if masking.reauthorize_plan(reader, &plan).await.is_err() {
            service
                .record_access(reader, &query, AccessOutcome::Denied)
                .await
                .map_err(|_| "audit_unavailable")?;
            return Err("privacy_policy_changed");
        }
        if service.reauthorize(reader, &query).await.is_err() {
            service
                .record_access(reader, &query, AccessOutcome::Denied)
                .await
                .map_err(|_| "audit_unavailable")?;
            return Err("authorization_denied");
        }
        service
            .record_access(reader, &query, AccessOutcome::Succeeded)
            .await
            .map_err(|_| "audit_unavailable")?;
        // An audit round trip must not allow a concurrently withdrawn grant to
        // release a saved result. The second check still uses the opaque proof.
        if service.reauthorize(reader, &query).await.is_err() {
            service
                .record_access(reader, &query, AccessOutcome::Denied)
                .await
                .map_err(|_| "audit_unavailable")?;
            return Err("authorization_denied");
        }
        if masking.reauthorize_plan(reader, &plan).await.is_err() {
            service
                .record_access(reader, &query, AccessOutcome::Denied)
                .await
                .map_err(|_| "audit_unavailable")?;
            return Err("privacy_policy_changed");
        }
        if let Err(reason) = self.verify_saved_domain(reader, job).await {
            service
                .record_access(reader, &query, AccessOutcome::Denied)
                .await
                .map_err(|_| "audit_unavailable")?;
            return Err(reason);
        }
        Ok(protected)
    }

    async fn verify_saved_domain(
        &self,
        reader: &AuthIdentity,
        job: &ProductJob,
    ) -> Result<(), &'static str> {
        self.authorize_read(reader, &job.source_kind)
            .await
            .map_err(|_| "authorization_denied")?;
        if job.source_kind == "recognition_task"
            || crate::services::metrics::is_calculable(&job.source_kind)
        {
            let metadata = self.metadata.as_ref().ok_or("authorization_unconfigured")?;
            if job.source_kind == "recognition_task" {
                crate::services::sensitive_recognition::verify_current(
                    self.resources.as_ref(),
                    metadata.as_ref(),
                    job,
                )
                .await
                .map_err(|_| "definition_withdrawn")?;
            } else {
                crate::services::metrics::verify_current(
                    self.resources.as_ref(),
                    metadata.as_ref(),
                    self.standards.as_deref(),
                    job,
                )
                .await
                .map_err(|_| "definition_withdrawn")?;
            }
        }
        if job.source_kind == "data_api" {
            self.authorize_read(reader, "data_api")
                .await
                .map_err(|_| "authorization_denied")?;
            let current = self
                .resources
                .get(&job.tenant_id, "data_api", &job.source_id)
                .await
                .map_err(|_| "authorization_denied")?
                .ok_or("publication_withdrawn")?;
            if current.status != "published"
                || current.version != job.source_version
                || current.properties != job.definition_snapshot["properties"]
            {
                return Err("publication_withdrawn");
            }
        }
        if ["bi_report", "data_api"].contains(&job.source_kind.as_str()) {
            let frozen = &job.definition_snapshot["metadata"];
            let id = frozen["id"].as_str().ok_or("definition_invalid")?;
            let version = frozen["version"].as_i64().ok_or("definition_invalid")?;
            let expected: MetadataEngineBinding = serde_json::from_value(frozen["binding"].clone())
                .map_err(|_| "definition_invalid")?;
            let repo = self.metadata.as_ref().ok_or("authorization_unconfigured")?;
            let current = repo
                .verified_binding(&job.tenant_id, id, version)
                .await
                .map_err(|_| "authorization_denied")?;
            if current.as_ref() != Some(&expected)
                || !binding_matches(&expected, &frozen["properties"])
            {
                return Err("metadata_proof_invalid");
            }
        }
        Ok(())
    }
}

impl ProductJobView {
    pub(super) fn summary(mut job: ProductJob) -> Self {
        let node_access = job
            .nodes
            .iter_mut()
            .map(|node| {
                let available = node.result.take().is_some();
                NodeAccess {
                    access: if available {
                        ResultAccess::Summary
                    } else {
                        ResultAccess::NotAvailable
                    },
                    reason: available.then_some("detail_authorization_required"),
                }
            })
            .collect();
        let available = job.result.take().is_some();
        sanitize_definition(&mut job);
        Self {
            job,
            result_access: if available {
                ResultAccess::Summary
            } else {
                ResultAccess::NotAvailable
            },
            result_access_reason: available.then_some("detail_authorization_required"),
            node_access,
        }
    }
}

fn sanitize_definition(job: &mut ProductJob) {
    job.definition_snapshot = json!({});
    job.parameters = json!({});
    for node in &mut job.nodes {
        node.definition = json!({});
    }
}

fn metadata_summary(value: Option<Value>) -> Option<Value> {
    serde_json::from_value::<MetadataApplyResult>(value?)
        .ok()
        .filter(|result| result.reflection_committed)
        .and_then(|result| serde_json::to_value(result).ok())
}

fn control_summary(job: &ProductJob) -> Option<Value> {
    if job.source_kind == "metadata_collection" {
        return metadata_summary(job.result.clone());
    }
    // Construct summaries from actual node records rather than copying unknown
    // stored keys (which could contain row data or source query parameters).
    if job.status != ProductJobStatus::Succeeded
        || job
            .nodes
            .iter()
            .any(|node| node.status != ProductJobNodeStatus::Succeeded)
    {
        return None;
    }
    if job.source_kind == "quality_task" {
        Some(
            json!({"execution_mode":"read_only","succeeded_checks":job.nodes.len(),
            "counts_scope":"per_rule_node_results"}),
        )
    } else if job.source_kind == "recognition_task" {
        let result = serde_json::from_value::<
            dts_backend::repo::sensitive_findings::SensitiveScanResult,
        >(job.nodes.first()?.result.clone()?)
        .ok()?;
        if !result.producer_committed {
            return None;
        }
        Some(
            json!({"execution_mode":"bounded_sensitive_scan","producer_committed":true,"evaluation_scope":result.evaluation_scope,"sampled_rows":result.sampled_rows,"finding_count":result.finding_ids.len(),"classification_enforced":false}),
        )
    } else {
        let returned_rows = job
            .nodes
            .iter()
            .map(|node| node.result.as_ref()?["row_count"].as_u64())
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .try_fold(0_u64, u64::checked_add)?;
        Some(
            json!({"execution_mode":"read_only","succeeded_nodes":job.nodes.len(),
            "returned_rows":returned_rows}),
        )
    }
}
