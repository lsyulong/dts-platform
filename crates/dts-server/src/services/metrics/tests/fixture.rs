use super::*;
use chrono::{Duration, Utc};
use dts_backend::repo::metadata_catalog::{MetadataColumn, MetadataEngineBinding, MetadataTable};
use serde_json::json;
use std::collections::BTreeMap;

pub(super) fn metadata() -> FrozenMetadata {
    let table = MetadataTable {
        table_catalog: "lake".into(),
        schema_name: "main".into(),
        table_name: "orders".into(),
        table_type: "BASE TABLE".into(),
        columns: [
            ("id", "BIGINT", false),
            ("event_time", "TIMESTAMPTZ", false),
            ("地区", "VARCHAR", true),
            ("销售金额", "DECIMAL(18,2)", true),
        ]
        .into_iter()
        .enumerate()
        .map(|(ordinal, (name, ty, nullable))| MetadataColumn {
            name: name.into(),
            data_type: ty.into(),
            nullable,
            ordinal_position: ordinal as u32 + 1,
        })
        .collect(),
    };
    FrozenMetadata {
        id: "metadata".into(),
        version: 1,
        properties: table.properties("tenant").unwrap(),
        columns: table.columns.clone(),
        binding: MetadataEngineBinding {
            engine: "swanlake".into(),
            tenant_id: "tenant".into(),
            object_version: 1,
            table_catalog: table.table_catalog.clone(),
            schema_name: table.schema_name.clone(),
            table_name: table.table_name.clone(),
            source_fingerprint: table.fingerprint().unwrap(),
        },
    }
}
pub(super) fn plan() -> MetricPlan {
    MetricPlan {
        model_id: "model".into(),
        model_version: 3,
        grain: vec!["id".into()],
        dimensions: vec!["地区".into()],
        time_column: "event_time".into(),
        time_timezone: "Asia/Shanghai".into(),
        measures: vec![MetricMeasure {
            code: "m_amount".into(),
            aggregate: "SUM".into(),
            argument: Some("\"销售金额\"".into()),
            modifier: None,
            period: None,
            unit: None,
        }],
        expression: "\"m_amount\"".into(),
        unit: None,
        dimension: BTreeMap::new(),
        numeric_semantics: "engine_native_exact".into(),
    }
}
pub(super) fn snapshot() -> MetricSnapshot {
    let metadata = metadata();
    let plan = plan();
    let request = MetricRunRequest {
        expected_version: 2,
        idempotency_key: "metric-test".into(),
        window_start: Some("2026-09-01T00:00:00Z".parse().unwrap()),
        window_end: Some("2026-10-01T00:00:00Z".parse().unwrap()),
        as_of: None,
        row_limit: 2,
    };
    let properties = json!({"process_id":"process","model_id":"model","metadata_object_id":"metadata","expression":"SUM(\"销售金额\")","grain":"订单","grain_columns":["id"],"time_column":"event_time","time_timezone":"Asia/Shanghai","dimensions":["地区"]});
    let publication = MetricPublication {
        format_version: 1,
        executor: "metric_aggregate_v1".into(),
        tenant_id: "tenant".into(),
        source_kind: "atomic_metric".into(),
        source_id: "metric".into(),
        source_version: 2,
        properties: properties.clone(),
        metadata: Some(metadata.clone()),
        references: vec![],
        plan: Some(plan.clone()),
        period: None,
        modifier: None,
    };
    let mut properties = properties;
    properties["publication"] = json!(publication);
    MetricSnapshot {
        format_version: 1,
        executor: "metric_aggregate_v1".into(),
        tenant_id: "tenant".into(),
        source_kind: "atomic_metric".into(),
        source_id: "metric".into(),
        source_version: 2,
        properties,
        metadata,
        publication,
        window: window::resolve(&plan, &request).unwrap(),
        request,
    }
}
pub(super) fn job(snapshot: &MetricSnapshot) -> ProductJob {
    let node = compiler::node(snapshot).unwrap();
    let now = Utc::now();
    ProductJob {
        id: "job".into(),
        tenant_id: snapshot.tenant_id.clone(),
        source_kind: snapshot.source_kind.clone(),
        source_id: snapshot.source_id.clone(),
        source_version: snapshot.source_version,
        actor_id: "actor".into(),
        action: "execute_metric".into(),
        idempotency_key: snapshot.request.idempotency_key.clone(),
        definition_snapshot: json!(snapshot),
        parameters: json!(snapshot.request),
        status: ProductJobStatus::Running,
        cancel_requested: false,
        cancel_requested_by: None,
        can_retry: true,
        attempt: 1,
        lease_owner: Some("worker".into()),
        lease_fence: 1,
        lease_expires_at: Some(now + Duration::seconds(30)),
        result: None,
        error: None,
        reconciled_by: None,
        created_at: now,
        updated_at: now,
        started_at: Some(now),
        finished_at: None,
        nodes: vec![ProductJobNode {
            id: node.id,
            name: node.name,
            definition: node.definition,
            depends_on: node.depends_on,
            status: ProductJobNodeStatus::Running,
            attempt: 1,
            result: None,
            error: None,
            started_at: Some(now),
            finished_at: None,
        }],
    }
}
