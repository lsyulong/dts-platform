// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use dts_backend::repo::{
    metadata_catalog::{MetadataApplyParams, MetadataScope, MetadataTable, binding_matches},
    product_job::*,
    sensitive_findings::{SensitiveCommitParams, SensitiveScanResult},
};
use serde_json::{Value, json};
use tokio::{sync::watch, task::JoinHandle};

use crate::models::{auth::AuthIdentity, error::AppError};
use crate::services::data_access::AccessOutcome;

use super::query_guard::{GuardedNodeOutput, JobQueryGuard};
use super::{LEASE_SECONDS, ProductJobService, SOURCE_KINDS, definition, executable_source};

pub struct ProductJobWorkerHandle {
    stop: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl ProductJobWorkerHandle {
    pub async fn shutdown(mut self) -> Result<(), AppError> {
        let _ = self.stop.send(true);
        if let Some(handle) = self.handle.take() {
            handle
                .await
                .map_err(|_| AppError::Internal("只读执行器退出失败".into()))?;
        }
        Ok(())
    }
}

impl Drop for ProductJobWorkerHandle {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(handle) = &self.handle {
            handle.abort();
        }
    }
}

enum NodeQuery {
    Completed(Value),
    Collected(Vec<MetadataTable>),
    Recognized(SensitiveScanResult),
    Failed(String),
    Cancelled,
}

impl ProductJobService {
    /// The owning application must retain this handle and explicitly shut it down.
    pub fn spawn_worker(&self) -> Option<ProductJobWorkerHandle> {
        self.access.tenant_id.as_ref()?;
        let service = self.clone();
        let worker_id = format!("readonly-{}", uuid::Uuid::new_v4());
        let (stop, mut stopped) = watch::channel(false);
        let handle = tokio::spawn(async move {
            loop {
                if *stopped.borrow() {
                    break;
                }
                if let Err(error) = service.claim_and_run(&worker_id, Some(&stopped)).await {
                    tracing::warn!(error = %error, "read-only execution worker failed");
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {},
                    _ = stopped.changed() => break,
                }
            }
        });
        Some(ProductJobWorkerHandle {
            stop,
            handle: Some(handle),
        })
    }

    pub async fn run_once(&self) -> Result<bool, AppError> {
        self.claim_and_run(&format!("readonly-once-{}", uuid::Uuid::new_v4()), None)
            .await
    }

    async fn claim_and_run(
        &self,
        worker: &str,
        stop: Option<&watch::Receiver<bool>>,
    ) -> Result<bool, AppError> {
        let Some(tenant) = &self.access.tenant_id else {
            return Ok(false);
        };
        let Some(job) = self
            .jobs
            .claim(&ClaimProductJobParams {
                tenant_id: tenant.clone(),
                worker_id: worker.into(),
                lease_seconds: LEASE_SECONDS,
                source_kinds: SOURCE_KINDS.iter().map(|kind| (*kind).into()).collect(),
            })
            .await?
        else {
            return Ok(false);
        };
        let lease = ProductJobLease {
            worker_id: job
                .lease_owner
                .clone()
                .ok_or_else(|| AppError::Internal("已领取运行缺少执行器身份".into()))?,
            fence: job.lease_fence,
        };
        self.run_claimed(job, &lease, stop).await?;
        Ok(true)
    }

    async fn run_claimed(
        &self,
        mut job: ProductJob,
        lease: &ProductJobLease,
        stop: Option<&watch::Receiver<bool>>,
    ) -> Result<(), AppError> {
        let queries = match compile_claimed(&job) {
            Ok(queries) => queries,
            Err(error) => return self.fail_job(&job, lease, protected_error(&error)).await,
        };
        let budget = (256 * 1024 - 8192) / job.nodes.len().max(1);
        loop {
            if stop.is_some_and(|stop| *stop.borrow()) {
                return Ok(());
            }
            job = self.jobs.heartbeat(&heartbeat(&job, lease)).await?;
            if job.cancel_requested {
                return self
                    .finish_job(&job, lease, ProductJobOutcome::Cancelled, None, None)
                    .await;
            }
            if let Err(error) = self.authorize_job(&job).await {
                return self.fail_job(&job, lease, protected_error(&error)).await;
            }
            let next = job.nodes.iter().find(|node| {
                node.status == ProductJobNodeStatus::Pending
                    && node.depends_on.iter().all(|dependency| {
                        job.nodes.iter().any(|upstream| {
                            &upstream.id == dependency
                                && upstream.status == ProductJobNodeStatus::Succeeded
                        })
                    })
            });
            let Some(node) = next else {
                return self.complete_or_fail(&job, lease).await;
            };
            let node_id = node.id.clone();
            let node_definition = node.definition.clone();
            let sql = queries
                .iter()
                .find(|(id, _)| id == &node_id)
                .map(|(_, sql)| sql.as_str())
                .ok_or_else(|| AppError::Internal("运行节点缺少已验证的 SQL".into()))?;
            self.jobs
                .start_node(&StartProductJobNodeParams {
                    tenant_id: job.tenant_id.clone(),
                    job_id: job.id.clone(),
                    node_id: node_id.clone(),
                    lease: lease.clone(),
                })
                .await?;
            match self
                .query_node(&job, lease, &node_id, &node_definition, sql, budget)
                .await?
            {
                NodeQuery::Completed(result) => {
                    self.finish_node(
                        &job,
                        lease,
                        &node_id,
                        ProductJobOutcome::Succeeded,
                        Some(result),
                        None,
                    )
                    .await?;
                },
                NodeQuery::Recognized(result) => {
                    let repo = self
                        .sensitive_findings
                        .as_ref()
                        .ok_or_else(|| AppError::BadRequest("真实敏感发现仓储未配置".into()))?;
                    if let Err(error) = repo
                        .commit_scan(&SensitiveCommitParams {
                            tenant_id: job.tenant_id.clone(),
                            actor_id: job.actor_id.clone(),
                            job_id: job.id.clone(),
                            node_id: node_id.clone(),
                            lease: lease.clone(),
                            snapshot: job.definition_snapshot.clone(),
                            result,
                        })
                        .await
                    {
                        let fresh = self.jobs.heartbeat(&heartbeat(&job, lease)).await?;
                        let cancelled = fresh.cancel_requested;
                        let reason = (!cancelled).then(|| protected_error(&error.into()));
                        let outcome = if cancelled {
                            ProductJobOutcome::Cancelled
                        } else {
                            ProductJobOutcome::Failed
                        };
                        self.finish_node(&job, lease, &node_id, outcome, None, reason.clone())
                            .await?;
                        return self.finish_job(&job, lease, outcome, None, reason).await;
                    }
                    // The producer commits both node and job success with findings.
                    return Ok(());
                },
                NodeQuery::Collected(tables) => {
                    if let Err(error) = self.reflect_metadata(&job, lease, &node_id, tables).await {
                        let fresh = self.jobs.heartbeat(&heartbeat(&job, lease)).await?;
                        if fresh.cancel_requested {
                            self.finish_node(
                                &job,
                                lease,
                                &node_id,
                                ProductJobOutcome::Cancelled,
                                None,
                                None,
                            )
                            .await?;
                            return self
                                .finish_job(&job, lease, ProductJobOutcome::Cancelled, None, None)
                                .await;
                        }
                        let error = protected_error(&error);
                        self.finish_node(
                            &job,
                            lease,
                            &node_id,
                            ProductJobOutcome::Failed,
                            None,
                            Some(error.clone()),
                        )
                        .await?;
                        return self.fail_job(&job, lease, error).await;
                    }
                },
                NodeQuery::Failed(error) => {
                    self.finish_node(
                        &job,
                        lease,
                        &node_id,
                        ProductJobOutcome::Failed,
                        None,
                        Some(error.clone()),
                    )
                    .await?;
                    return self.fail_job(&job, lease, error).await;
                },
                NodeQuery::Cancelled => {
                    self.finish_node(
                        &job,
                        lease,
                        &node_id,
                        ProductJobOutcome::Cancelled,
                        None,
                        None,
                    )
                    .await?;
                    return self
                        .finish_job(&job, lease, ProductJobOutcome::Cancelled, None, None)
                        .await;
                },
            }
        }
    }

    async fn query_node(
        &self,
        job: &ProductJob,
        lease: &ProductJobLease,
        node_id: &str,
        node_definition: &Value,
        sql: &str,
        budget: usize,
    ) -> Result<NodeQuery, AppError> {
        let guard = match JobQueryGuard::prepare(self, job, node_id, sql).await {
            Ok(guard) => guard,
            Err(error) => return Ok(NodeQuery::Failed(protected_error(&error))),
        };
        if let Err(error) = guard.reauthorize().await {
            return Ok(NodeQuery::Failed(protected_error(&error)));
        }
        let execution = tokio::time::timeout(
            Duration::from_secs(super::runtime_configuration::query_timeout(job)?),
            self.query.execute(sql),
        );
        tokio::pin!(execution);
        let mut heartbeat_timer = tokio::time::interval(Duration::from_secs(5));
        heartbeat_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        heartbeat_timer.tick().await;
        loop {
            tokio::select! {
                result = &mut execution => {
                    let fresh = match self.jobs.heartbeat(&heartbeat(job, lease)).await {
                        Ok(fresh)=>fresh,
                        Err(error)=>{guard.record(AccessOutcome::Failed).await?;return Err(error.into());},
                    };
                    if fresh.cancel_requested {guard.record(AccessOutcome::Failed).await?;return Ok(NodeQuery::Cancelled); }
                    if let Err(error) = self.authorize_job(&fresh).await {guard.record(AccessOutcome::Denied).await?;return Ok(NodeQuery::Failed(protected_error(&error))); }
                    if let Err(error)=guard.reauthorize().await {return Ok(NodeQuery::Failed(protected_error(&error)));}
                    let observed=match result {
                        Ok(Ok(result)) => match guard.apply_output(job,result) {
                            Ok(result)=>self.observed_result(job,node_definition,result,budget),
                            Err(error)=>NodeQuery::Failed(protected_error(&error)),
                        },
                        Ok(Err(error)) => NodeQuery::Failed(protected_error(&error)),
                        Err(_) => NodeQuery::Failed("只读查询执行超过 65 秒上限".into()),
                    };
                    let outcome=if matches!(&observed,NodeQuery::Completed(_)|NodeQuery::Collected(_)|NodeQuery::Recognized(_)) {AccessOutcome::Succeeded} else {AccessOutcome::Failed};
                    if let Err(error)=guard.record(outcome).await {return Ok(NodeQuery::Failed(protected_error(&error)));}
                    if matches!(&observed,NodeQuery::Completed(_)|NodeQuery::Collected(_)|NodeQuery::Recognized(_)) {
                        if let Err(error)=guard.reauthorize().await {return Ok(NodeQuery::Failed(protected_error(&error)));}
                        let final_job=self.jobs.heartbeat(&heartbeat(job,lease)).await?;
                        if final_job.cancel_requested {guard.record(AccessOutcome::Failed).await?;return Ok(NodeQuery::Cancelled);}
                        if let Err(error)=self.authorize_job(&final_job).await {guard.record(AccessOutcome::Denied).await?;return Ok(NodeQuery::Failed(protected_error(&error)));}
                    }
                    return Ok(observed);
                },
                _ = heartbeat_timer.tick() => {
                    let renewal=tokio::time::timeout(Duration::from_secs(10), self.jobs.heartbeat(&heartbeat(job, lease))).await
                        .map_err(|_| AppError::Internal("执行租约续期超时，已停止等待只读查询".into()))
                        .and_then(|result|result.map_err(AppError::from));
                    let fresh=match renewal {Ok(fresh)=>fresh,Err(error)=>{guard.record(AccessOutcome::Failed).await?;return Err(error);}};
                    if fresh.cancel_requested {guard.record(AccessOutcome::Failed).await?; return Ok(NodeQuery::Cancelled); }
                    if let Err(error) = self.authorize_job(&fresh).await {guard.record(AccessOutcome::Denied).await?;return Ok(NodeQuery::Failed(protected_error(&error))); }
                    if let Err(error)=guard.reauthorize().await {return Ok(NodeQuery::Failed(protected_error(&error)));}
                },
            }
        }
    }

    async fn authorize_job(&self, job: &ProductJob) -> Result<(), AppError> {
        if job.source_kind == "metadata_collection" && self.metadata.is_none() {
            return Err(AppError::BadRequest("元数据反射仓储未配置".into()));
        }
        if job.source_kind == "recognition_task" && self.sensitive_findings.is_none() {
            return Err(AppError::BadRequest("真实敏感发现仓储未配置".into()));
        }
        let user = self
            .auth
            .user_repo
            .find_detail_by_id(&job.actor_id)
            .await?
            .filter(|user| user.tenant_id == job.tenant_id)
            .ok_or_else(|| AppError::Unauthorized("运行提交者不存在".into()))?;
        let actor = AuthIdentity {
            sub: user.id,
            tenant_id: user.tenant_id,
            username: user.username,
        };
        self.authorize_execute(&actor, &job.source_kind).await?;
        self.verify_runtime(&actor, job, true).await?;
        let source = self
            .resources
            .get(&job.tenant_id, &job.source_kind, &job.source_id)
            .await?
            .ok_or_else(|| AppError::NotFound("运行来源不存在".into()))?;
        if !executable_source(&source)
            || source.version != job.source_version
            || source.properties != job.definition_snapshot["properties"]
        {
            return Err(AppError::Conflict("运行定义已经撤回或版本改变".into()));
        }
        if ["quality_task", "bi_report", "data_api"].contains(&job.source_kind.as_str()) {
            let frozen = &job.definition_snapshot["metadata"];
            let id = frozen["id"]
                .as_str()
                .ok_or_else(|| AppError::BadRequest("运行缺少实际元数据身份".into()))?;
            let version = frozen["version"]
                .as_i64()
                .ok_or_else(|| AppError::BadRequest("运行缺少实际元数据版本".into()))?;
            let repo = self
                .metadata
                .as_ref()
                .ok_or_else(|| AppError::BadRequest("元数据内部证明仓储未配置".into()))?;
            let proof = repo
                .verified_binding(&job.tenant_id, id, version)
                .await?
                .filter(|proof| binding_matches(proof, &frozen["properties"]))
                .ok_or_else(|| AppError::Conflict("来源元数据内部证明已改变或失效".into()))?;
            if proof.engine != "swanlake" || proof.tenant_id != job.tenant_id {
                return Err(AppError::Forbidden("来源引擎证明不属于当前租户".into()));
            }
        }
        if job.source_kind == "recognition_task"
            || crate::services::metrics::is_calculable(&job.source_kind)
        {
            let metadata = self
                .metadata
                .as_ref()
                .ok_or_else(|| AppError::BadRequest("当前运行来源未绑定实际元数据仓储".into()))?;
            if job.source_kind == "recognition_task" {
                crate::services::sensitive_recognition::verify_current(
                    self.resources.as_ref(),
                    metadata.as_ref(),
                    job,
                )
                .await?;
            } else {
                crate::services::metrics::verify_current(
                    self.resources.as_ref(),
                    metadata.as_ref(),
                    self.standards.as_deref(),
                    job,
                )
                .await?;
            }
        }
        Ok(())
    }

    fn observed_result(
        &self,
        job: &ProductJob,
        node: &Value,
        output: GuardedNodeOutput,
        budget: usize,
    ) -> NodeQuery {
        let (result, receipt) = match output {
            GuardedNodeOutput::Recognition(result) if job.source_kind == "recognition_task" => {
                return NodeQuery::Recognized(result);
            },
            GuardedNodeOutput::Recognition(_) => {
                return NodeQuery::Failed("执行器返回不匹配的真实敏感计数".into());
            },
            GuardedNodeOutput::Api(value) if job.source_kind == "data_api" => {
                return bounded_value(value, budget);
            },
            GuardedNodeOutput::Api(_) => {
                return NodeQuery::Failed("执行器返回不匹配的受保护API结果".into());
            },
            GuardedNodeOutput::Query(result, receipt) => (result, receipt),
        };
        if job.source_kind == "metadata_collection" {
            let decoded =
                serde_json::from_value::<MetadataScope>(job.definition_snapshot["scope"].clone())
                    .map_err(|_| AppError::BadRequest("采集固定范围无效".into()))
                    .and_then(|scope| crate::services::metadata::decode_result(&scope, result));
            match decoded {
                Ok(tables) => NodeQuery::Collected(tables),
                Err(error) => NodeQuery::Failed(protected_error(&error)),
            }
        } else {
            bounded_result(job, node, result, budget, receipt)
        }
    }

    async fn reflect_metadata(
        &self,
        job: &ProductJob,
        lease: &ProductJobLease,
        node: &str,
        tables: Vec<MetadataTable>,
    ) -> Result<(), AppError> {
        let repo = self
            .metadata
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("元数据反射仓储未配置".into()))?;
        let scope = serde_json::from_value(job.definition_snapshot["scope"].clone())
            .map_err(|_| AppError::BadRequest("采集范围无效".into()))?;
        repo.apply_snapshot(&MetadataApplyParams {
            tenant_id: job.tenant_id.clone(),
            actor_id: job.actor_id.clone(),
            collection_id: job.source_id.clone(),
            collection_version: job.source_version,
            collection_properties: job.definition_snapshot["properties"].clone(),
            scope,
            tables,
            job_id: job.id.clone(),
            node_id: node.into(),
            lease: lease.clone(),
        })
        .await?;
        Ok(())
    }

    async fn finish_node(
        &self,
        job: &ProductJob,
        lease: &ProductJobLease,
        node: &str,
        outcome: ProductJobOutcome,
        result: Option<Value>,
        error: Option<String>,
    ) -> Result<(), AppError> {
        self.jobs
            .finish_node(&FinishProductJobNodeParams {
                tenant_id: job.tenant_id.clone(),
                job_id: job.id.clone(),
                node_id: node.into(),
                lease: lease.clone(),
                outcome,
                result,
                error,
            })
            .await?;
        Ok(())
    }

    async fn finish_job(
        &self,
        job: &ProductJob,
        lease: &ProductJobLease,
        outcome: ProductJobOutcome,
        result: Option<Value>,
        error: Option<String>,
    ) -> Result<(), AppError> {
        self.jobs
            .finish(&FinishProductJobParams {
                tenant_id: job.tenant_id.clone(),
                job_id: job.id.clone(),
                lease: lease.clone(),
                outcome,
                result,
                error,
            })
            .await?;
        Ok(())
    }

    async fn fail_job(
        &self,
        job: &ProductJob,
        lease: &ProductJobLease,
        error: String,
    ) -> Result<(), AppError> {
        self.finish_job(job, lease, ProductJobOutcome::Failed, None, Some(error))
            .await
    }

    async fn complete_or_fail(
        &self,
        job: &ProductJob,
        lease: &ProductJobLease,
    ) -> Result<(), AppError> {
        if !job
            .nodes
            .iter()
            .all(|node| node.status == ProductJobNodeStatus::Succeeded)
        {
            return self
                .fail_job(job, lease, "节点存在失败、跳过或不满足依赖的状态".into())
                .await;
        }
        if job.source_kind == "quality_task" {
            return self.finish_job(job, lease, ProductJobOutcome::Succeeded,
                Some(json!({"execution_mode":"read_only","succeeded_checks":job.nodes.len(),"counts_scope":"per_rule_node_results"})), None).await;
        }
        if job.source_kind == "metadata_collection" {
            let result = job
                .nodes
                .first()
                .and_then(|node| node.result.clone())
                .filter(|value| value["reflection_committed"] == true)
                .ok_or_else(|| AppError::Internal("成功采集节点缺少原子反射提交结果".into()))?;
            return self
                .finish_job(job, lease, ProductJobOutcome::Succeeded, Some(result), None)
                .await;
        }
        let rows: u64 = job
            .nodes
            .iter()
            .filter_map(|node| node.result.as_ref()?.get("row_count")?.as_u64())
            .sum();
        self.finish_job(job, lease, ProductJobOutcome::Succeeded, Some(json!({"execution_mode":"read_only","succeeded_nodes":job.nodes.len(),"returned_rows":rows})), None).await
    }
}

fn heartbeat(job: &ProductJob, lease: &ProductJobLease) -> HeartbeatProductJobParams {
    HeartbeatProductJobParams {
        tenant_id: job.tenant_id.clone(),
        job_id: job.id.clone(),
        lease: lease.clone(),
        lease_seconds: LEASE_SECONDS,
    }
}

fn bounded_result(
    job: &ProductJob,
    node: &Value,
    result: crate::services::query_svc::QueryResult,
    budget: usize,
    receipt: Option<crate::services::data_masking::MaskingReceipt>,
) -> NodeQuery {
    let result = if job.source_kind == "quality_task" {
        crate::services::data_quality::map_node_result(node, result)
    } else if job.source_kind == "bi_report" {
        crate::services::bi_report::map_node_result(job, result)
    } else if job.source_kind == "data_api" {
        crate::services::data_services::map_node_result(job, result)
    } else if crate::services::metrics::is_calculable(&job.source_kind) {
        crate::services::metrics::map_node_result(job, result)
    } else {
        serde_json::to_value(result).map_err(|_| AppError::Internal("真实查询结果无法编码".into()))
    };
    let mut result = match result {
        Ok(result) => result,
        Err(error) => return NodeQuery::Failed(protected_error(&error)),
    };
    if job.source_kind != "quality_task" {
        if let Some(receipt) = receipt {
            result["privacy"] = json!(receipt);
        }
    }
    bounded_value(result, budget)
}

fn bounded_value(result: Value, budget: usize) -> NodeQuery {
    match serde_json::to_vec(&result).map(|bytes| (result, bytes.len())) {
        Ok((result, size)) if size <= budget => NodeQuery::Completed(result),
        Ok(_) => {
            NodeQuery::Failed("查询真实结果超过运行存储限额，请在 SQL 中限制返回列和行".into())
        },
        Err(_) => NodeQuery::Failed("查询真实结果无法编码为 JSON".into()),
    }
}

pub(super) fn compile_claimed(job: &ProductJob) -> Result<Vec<(String, String)>, AppError> {
    let queries = if job.source_kind == "quality_task" {
        crate::services::data_quality::prepare_claimed(job)?
    } else if job.source_kind == "metadata_collection" {
        crate::services::metadata::prepare_claimed(job)?
    } else if job.source_kind == "bi_report" {
        crate::services::bi_report::prepare_claimed(job)?
    } else if job.source_kind == "data_api" {
        crate::services::data_services::prepare_claimed(job)?
    } else if job.source_kind == "recognition_task" {
        crate::services::sensitive_recognition::prepare_claimed(job)?
    } else if crate::services::metrics::is_calculable(&job.source_kind) {
        crate::services::metrics::prepare_claimed(job)?
    } else {
        definition::prepare_claimed(job)?.queries
    };
    if !job.can_retry || queries.len() != job.nodes.len() {
        return Err(AppError::BadRequest("运行没有通过全部只读节点验证".into()));
    }
    for (_, sql) in &queries {
        crate::services::query_svc::validate_read_only_sql(sql)?;
    }
    Ok(queries)
}

fn protected_error(error: &AppError) -> String {
    match error {
        AppError::Unauthorized(_) => "运行提交者会话身份或账户已失效",
        AppError::Forbidden(_) => "运行提交者不再具有相应业务域执行权限或查询租户绑定无效",
        AppError::Conflict(_) => "执行定义已撤回、版本改变或执行权发生冲突",
        AppError::NotFound(_) => "执行定义或提交者已不存在",
        AppError::BadRequest(_) => {
            "只读查询、参数或定义不满足实际执行约束，请检查 SQL 与分析引擎状态"
        },
        AppError::Internal(_) => "只读执行发生服务错误，请检查受控服务日志",
        AppError::RateLimited { .. } => "调用超过当前分钟额度，请按服务端重试时间等待",
    }
    .into()
}
