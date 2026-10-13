mod composite;
use super::{bad, definition, expression, proof, types::*};
use crate::models::error::AppError;
use dts_backend::{
    ProductResource, ProductResourceRepo,
    repo::{metadata_catalog::MetadataCatalogRepo, standard_governance::StandardGovernanceRepo},
};
use ring::digest::{SHA256, digest};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

pub(super) fn hash(value: &MetricPublication) -> Result<String, AppError> {
    let bytes = serde_json::to_vec(value).map_err(|_| bad("指标发布证明无法编码"))?;
    Ok(digest_hex(&bytes))
}
pub(super) fn digest_hex(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn reference(value: &ProductResource) -> Result<FrozenReference, AppError> {
    let publication_hash = if value.properties.get("publication").is_some() {
        Some(hash(&proof::publication(value)?)?)
    } else {
        None
    };
    Ok(FrozenReference {
        kind: value.kind.clone(),
        id: value.id.clone(),
        version: value.version,
        code: value.code.clone(),
        status: value.status.clone(),
        properties: definition::definition_properties(&value.properties)?,
        references: value.references.clone(),
        publication_hash,
    })
}
fn add(target: &mut Vec<FrozenReference>, value: FrozenReference) -> Result<(), AppError> {
    if let Some(before) = target
        .iter()
        .find(|before| before.kind == value.kind && before.id == value.id)
    {
        if before != &value {
            return Err(bad("冻结过程中依赖版本发生变化"));
        }
    } else {
        if target.len() >= 64 {
            return Err(bad("指标发布依赖超过64个"));
        }
        target.push(value);
        target.sort_by(|a, b| (&a.kind, &a.id).cmp(&(&b.kind, &b.id)));
    }
    Ok(())
}
pub(super) struct Builder<'a> {
    pub resources: &'a dyn ProductResourceRepo,
    pub registry: &'a dyn MetadataCatalogRepo,
    pub standards: Option<&'a dyn StandardGovernanceRepo>,
    cache: Mutex<BTreeMap<(String, String, i64), MetricPublication>>,
    visits: AtomicUsize,
}
impl Builder<'_> {
    pub(super) fn new<'a>(
        resources: &'a dyn ProductResourceRepo,
        registry: &'a dyn MetadataCatalogRepo,
        standards: Option<&'a dyn StandardGovernanceRepo>,
    ) -> Builder<'a> {
        Builder {
            resources,
            registry,
            standards,
            cache: Mutex::new(BTreeMap::new()),
            visits: AtomicUsize::new(0),
        }
    }
    async fn dependency(
        &self,
        tenant: &str,
        kind: &str,
        id: &str,
        refs: &mut Vec<FrozenReference>,
    ) -> Result<ProductResource, AppError> {
        let value = proof::get(self.resources, tenant, kind, id).await?;
        if matches!(value.status.as_str(), "inactive" | "deleted") {
            return Err(bad("指标依赖已停用"));
        }
        add(refs, reference(&value)?)?;
        Ok(value)
    }
    async fn unit(
        &self,
        tenant: &str,
        id: Option<&str>,
        version: Option<i64>,
        refs: &mut Vec<FrozenReference>,
    ) -> Result<Option<UnitDefinition>, AppError> {
        if let Some((unit, reference)) =
            super::standard_dependencies::unit(self.standards, tenant, id, version).await?
        {
            add(refs, reference)?;
            Ok(Some(unit))
        } else {
            Ok(None)
        }
    }
    async fn child(
        &self,
        tenant: &str,
        kind: &str,
        id: &str,
        refs: &mut Vec<FrozenReference>,
        path: BTreeSet<(String, String)>,
    ) -> Result<MetricPublication, AppError> {
        let resource = proof::get(self.resources, tenant, kind, id).await?;
        let stored = proof::publication(&resource)?;
        if path.contains(&(kind.into(), id.into())) {
            return Err(bad("指标依赖存在循环"));
        }
        if self.visits.fetch_add(1, Ordering::Relaxed) >= 512 {
            return Err(bad("指标冻结依赖访问超过512次预算"));
        }
        let key = (kind.into(), id.into(), resource.version);
        let cached = self
            .cache
            .lock()
            .map_err(|_| bad("指标冻结缓存不可用"))?
            .get(&key)
            .cloned();
        let actual = if let Some(cached) = cached {
            cached
        } else {
            let actual = self.build(&resource, resource.version, path).await?;
            let mut cache = self.cache.lock().map_err(|_| bad("指标冻结缓存不可用"))?;
            if cache.len() >= 64
                || cache
                    .values()
                    .map(|value| serde_json::to_vec(value).map_or(192 * 1024, |bytes| bytes.len()))
                    .sum::<usize>()
                    + serde_json::to_vec(&actual)
                        .map_err(|_| bad("指标冻结不能编码"))?
                        .len()
                    > 8 * 1024 * 1024
            {
                return Err(bad("指标冻结依赖缓存超过容量"));
            }
            cache.insert(key, actual.clone());
            actual
        };
        if stored != actual {
            return Err(bad("依赖的发布证明或实际模型绑定已经改变，请重新发布"));
        }
        add(refs, reference(&resource)?)?;
        for value in &actual.references {
            add(refs, value.clone())?;
        }
        Ok(actual)
    }
    pub(super) fn build<'a>(
        &'a self,
        resource: &'a ProductResource,
        version: i64,
        mut path: BTreeSet<(String, String)>,
    ) -> Pin<Box<dyn Future<Output = Result<MetricPublication, AppError>> + Send + 'a>> {
        Box::pin(async move {
            if path.len() >= 16 || !path.insert((resource.kind.clone(), resource.id.clone())) {
                return Err(bad("指标依赖存在循环或超过16层"));
            }
            definition::validate_definition(&resource.kind, &resource.properties)?;
            let tenant = &resource.tenant_id;
            let mut publication = MetricPublication {
                format_version: 1,
                executor: "metric_aggregate_v1".into(),
                tenant_id: tenant.clone(),
                source_kind: resource.kind.clone(),
                source_id: resource.id.clone(),
                source_version: version,
                properties: definition::definition_properties(&resource.properties)?,
                metadata: None,
                references: vec![],
                plan: None,
                period: None,
                modifier: None,
            };
            match resource.kind.as_str() {
                "atomic_metric" => {
                    let definition: AtomicDefinition = definition::parse(&resource.properties)?;
                    let process = self
                        .dependency(
                            tenant,
                            "warehouse_process",
                            &definition.process_id,
                            &mut publication.references,
                        )
                        .await?;
                    if process.properties["grain"]
                        .as_str()
                        .is_none_or(|grain| grain.trim().is_empty())
                    {
                        return Err(bad("业务过程缺少实际业务粒度定义"));
                    }
                    if process.properties["grain"].as_str() != Some(definition.grain.as_str()) {
                        return Err(bad("原子指标业务粒度须与选定业务过程的真实粒度一致"));
                    }
                    let model = self
                        .dependency(
                            tenant,
                            "data_model",
                            &definition.model_id,
                            &mut publication.references,
                        )
                        .await?;
                    if let Some(domain) = model.properties["domain_id"]
                        .as_str()
                        .filter(|id| !id.is_empty())
                    {
                        if process.properties["domain_id"].as_str() != Some(domain) {
                            return Err(bad("业务过程与已发布模型的数据域不一致"));
                        }
                    }
                    let receipt = proof::model_receipt(self.resources, tenant, &model).await?;
                    super::standard_dependencies::model(self.standards, tenant, &model, &receipt)
                        .await?;
                    add(&mut publication.references, reference(&receipt)?)?;
                    let metadata = proof::metadata(
                        self.resources,
                        self.registry,
                        tenant,
                        &definition.metadata_object_id,
                    )
                    .await?;
                    proof::model_matches(&model, &receipt, &metadata, &definition)?;
                    let (aggregate, argument) =
                        expression::aggregate(&definition.expression, Some(&metadata.columns))?;
                    let unit = self
                        .unit(
                            tenant,
                            definition.unit_id.as_deref(),
                            definition.unit_version,
                            &mut publication.references,
                        )
                        .await?;
                    let code = format!("m_{}", &digest_hex(resource.id.as_bytes())[..16]);
                    let grain = definition
                        .grain_columns
                        .iter()
                        .map(|name| {
                            expression::column(name, &metadata.columns)
                                .map(|column| column.name.clone())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    publication.plan = Some(MetricPlan {
                        model_id: model.id,
                        model_version: model.version,
                        grain,
                        dimensions: proof::dimensions(&definition.dimensions, &metadata.columns)?,
                        time_column: expression::column(
                            &definition.time_column,
                            &metadata.columns,
                        )?
                        .name
                        .clone(),
                        time_timezone: definition.time_timezone,
                        numeric_semantics: if aggregate == "AVG"
                            || argument.as_ref().is_some_and(|value| value.contains(" / "))
                            || metadata.columns.iter().any(|column| {
                                crate::services::product_domain::warehouse::canonical_data_type(
                                    &column.data_type,
                                )
                                .is_ok_and(|ty| matches!(ty.as_str(), "FLOAT" | "DOUBLE"))
                                    && argument.as_ref().is_some_and(|value| {
                                        definition::quote(&column.name)
                                            .is_ok_and(|name| value.contains(&name))
                                    })
                            }) {
                            "engine_native_approximate"
                        } else {
                            "engine_native_exact"
                        }
                        .into(),
                        measures: vec![MetricMeasure {
                            code: code.clone(),
                            aggregate,
                            argument,
                            modifier: None,
                            period: None,
                            unit: unit.clone(),
                        }],
                        expression: definition::quote(&code)?,
                        dimension: expression::unit_dimension(unit.as_ref()),
                        unit,
                    });
                    publication.metadata = Some(metadata);
                },
                "metric_modifier" => {
                    let definition: ModifierDefinition = definition::parse(&resource.properties)?;
                    let metadata = proof::metadata(
                        self.resources,
                        self.registry,
                        tenant,
                        &definition.metadata_object_id,
                    )
                    .await?;
                    super::predicate::compile(
                        &definition.condition,
                        &definition.dimension,
                        &metadata.columns,
                    )?;
                    publication.modifier = Some(definition);
                    publication.metadata = Some(metadata);
                },
                "metric_period" => {
                    publication.period = Some(definition::parse(&resource.properties)?)
                },
                "derived_metric" => {
                    let definition: DerivedDefinition = definition::parse(&resource.properties)?;
                    let atomic = self
                        .child(
                            tenant,
                            "atomic_metric",
                            &definition.atomic_metric_id,
                            &mut publication.references,
                            path.clone(),
                        )
                        .await?;
                    let metadata = atomic
                        .metadata
                        .ok_or_else(|| bad("原子指标缺少真实元数据"))?;
                    let mut plan = atomic.plan.ok_or_else(|| bad("原子指标缺少冻结度量"))?;
                    let period = self
                        .child(
                            tenant,
                            "metric_period",
                            &definition.period_id,
                            &mut publication.references,
                            path.clone(),
                        )
                        .await?
                        .period
                        .ok_or_else(|| bad("周期尚未发布"))?;
                    if period.timezone != plan.time_timezone {
                        return Err(bad("周期时区须与原子指标时间字段的业务时区一致"));
                    }
                    let modifier = if let Some(id) = definition.modifier_id {
                        let child = self
                            .child(
                                tenant,
                                "metric_modifier",
                                &id,
                                &mut publication.references,
                                path.clone(),
                            )
                            .await?;
                        if child.metadata.as_ref() != Some(&metadata) {
                            return Err(bad("修饰词须绑定与原子指标相同版本的实际元数据"));
                        }
                        let value = child.modifier.ok_or_else(|| bad("修饰词尚未发布"))?;
                        Some(super::predicate::compile(
                            &value.condition,
                            &value.dimension,
                            &metadata.columns,
                        )?)
                    } else {
                        None
                    };
                    plan.dimensions = proof::dimensions(&definition.dimensions, &metadata.columns)?;
                    let code = format!("m_{}", &digest_hex(resource.id.as_bytes())[..16]);
                    if plan.measures.len() != 1 {
                        return Err(bad("派生指标只能从一个原子度量派生"));
                    }
                    plan.measures[0].code = code.clone();
                    plan.measures[0].modifier = modifier;
                    plan.measures[0].period = Some(period);
                    plan.expression = definition::quote(&code)?;
                    publication.plan = Some(plan);
                    publication.metadata = Some(metadata);
                },
                "composite_metric" => {
                    self.composite(resource, &mut publication, path.clone())
                        .await?
                },
                _ => return Err(bad("未知指标实体")),
            }
            if serde_json::to_vec(&publication)
                .map_err(|_| bad("指标发布证明无法编码"))?
                .len()
                > 192 * 1024
            {
                return Err(bad("指标冻结证明超过192KiB，请缩小定义或依赖"));
            }
            Ok(publication)
        })
    }
}
