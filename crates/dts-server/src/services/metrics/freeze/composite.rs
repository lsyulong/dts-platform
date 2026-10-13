use super::*;
impl Builder<'_> {
    pub(super) async fn composite(
        &self,
        resource: &ProductResource,
        publication: &mut MetricPublication,
        path: BTreeSet<(String, String)>,
    ) -> Result<(), AppError> {
        let tenant = &resource.tenant_id;
        let definition: CompositeDefinition = definition::parse(&resource.properties)?;
        let mut dependencies = vec![];
        for id in &definition.metric_ids {
            let mut matching = vec![];
            for kind in ["atomic_metric", "derived_metric", "composite_metric"] {
                if let Some(resource) = self.resources.get(tenant, kind, id).await? {
                    matching.push(resource);
                }
            }
            if matching.len() != 1 {
                return Err(bad("复合依赖必须唯一绑定当前租户实际指标"));
            }
            let resource = matching.remove(0);
            if dependencies
                .iter()
                .any(|(code, _): &(String, MetricPlan)| code.eq_ignore_ascii_case(&resource.code))
            {
                return Err(bad("复合依赖指标编码不能重复"));
            }
            let child = self
                .child(
                    tenant,
                    &resource.kind,
                    &resource.id,
                    &mut publication.references,
                    path.clone(),
                )
                .await?;
            if let Some(metadata) = &publication.metadata {
                if child.metadata.as_ref() != Some(metadata) {
                    return Err(bad("首轮复合指标仅支持相同实际表和元数据版本"));
                }
            } else {
                publication.metadata = child.metadata;
            }
            dependencies.push((
                resource.code,
                child.plan.ok_or_else(|| bad("复合依赖不是可执行指标"))?,
            ));
        }
        let mut plan = dependencies[0].1.clone();
        let mut measures = vec![];
        for (_, child) in &dependencies {
            if plan.model_id != child.model_id
                || plan.model_version != child.model_version
                || plan.grain != child.grain
                || plan.dimensions != child.dimensions
                || plan.time_column != child.time_column
                || plan.time_timezone != child.time_timezone
            {
                return Err(bad(
                    "复合指标依赖的模型版本、粒度、输出维度或时间口径不一致",
                ));
            }
            for measure in &child.measures {
                if let Some(before) = measures
                    .iter()
                    .find(|before: &&MetricMeasure| before.code == measure.code)
                {
                    if before != measure {
                        return Err(bad("冻结度量编码冲突"));
                    }
                } else {
                    measures.push(measure.clone());
                }
            }
        }
        if measures.len() > 32 {
            return Err(bad("复合度量超过32个"));
        }
        let (formula, dimension) = expression::formula(&definition.expression, &dependencies)?;
        let unit = self
            .unit(
                tenant,
                definition.unit_id.as_deref(),
                definition.unit_version,
                &mut publication.references,
            )
            .await?;
        if dimension != expression::unit_dimension(unit.as_ref()) {
            return Err(bad("复合输出单位与公式计算得到的计量维度不一致"));
        }
        if formula.contains(" / ")
            || dependencies
                .iter()
                .any(|(_, plan)| plan.numeric_semantics == "engine_native_approximate")
        {
            plan.numeric_semantics = "engine_native_approximate".into();
        }
        let reciprocal = crate::services::metrics::precision::reciprocal(
            unit.as_ref().map_or(1.0, |unit| unit.conversion_factor),
        )?;
        plan.measures = measures;
        plan.expression = if reciprocal == "1" {
            formula
        } else {
            format!("({formula}) * {reciprocal}")
        };
        plan.dimension = dimension;
        plan.unit = unit;
        publication.plan = Some(plan);
        Ok(())
    }
}
