use std::time::Duration;

use arrow_flight::{
    FlightData, FlightDescriptor,
    sql::{CommandStatementUpdate, DoPutUpdateResult, ProstMessageExt},
};
use prost::Message;
use serde_json::{Value, json};

use crate::{
    models::error::AppError,
    services::{product_domain::warehouse::generate_ddl, query_svc::QueryService},
};

#[derive(Clone)]
pub struct ModelPublisher {
    query: QueryService,
}

impl ModelPublisher {
    pub fn new(query: QueryService) -> Self {
        Self { query }
    }

    pub async fn reconcile(&self, properties: &Value) -> Result<Value, AppError> {
        let ddl = generate_ddl(properties)?;
        let binding = self.binding(properties).await?.ok_or_else(|| {
            AppError::Conflict("未找到原发布的实际基础表，结果待核对；不会重复建表".into())
        })?;
        self.verified_receipt(properties, ddl, binding, true, None)
            .await
    }

    async fn verified_receipt(
        &self,
        properties: &Value,
        ddl: String,
        binding: Value,
        reconciled: bool,
        count: Option<i64>,
    ) -> Result<Value, AppError> {
        let columns = self.columns(&binding).await?;
        verify_columns(properties, &columns)?;
        self.verify_constraints(properties, &binding).await?;
        let mut receipt = json!({"generated_ddl":ddl,"physical_columns":columns,"physical_binding":binding,"reconciled":reconciled});
        if let Some(count) = count {
            receipt["rows_affected"] = json!(count);
        }
        Ok(receipt)
    }

    pub async fn publish(&self, properties: &Value) -> Result<Value, AppError> {
        let ddl = generate_ddl(properties)?;
        if let Some(binding) = self.binding(properties).await? {
            return self
                .verified_receipt(properties, ddl, binding, true, None)
                .await;
        }
        let mut client = self.query.connect().await?;
        let command = CommandStatementUpdate {
            query: ddl.clone(),
            transaction_id: None,
        };
        let descriptor = FlightDescriptor::new_cmd(command.as_any().encode_to_vec());
        let stream = futures::stream::iter([FlightData {
            flight_descriptor: Some(descriptor),
            ..Default::default()
        }]);
        let operation = async {
            let mut response = client
                .inner_mut()
                .do_put(stream)
                .await
                .map_err(|_| {
                    AppError::Conflict("模型发布请求未得到确定结果，请核对物理结构后重试".into())
                })?
                .into_inner();
            let result = response
                .message()
                .await
                .map_err(|_| AppError::Conflict("模型发布响应中断，请核对物理结构后重试".into()))?
                .ok_or_else(|| {
                    AppError::Conflict("分析引擎未返回发布结果，请核对物理结构".into())
                })?;
            DoPutUpdateResult::decode(result.app_metadata)
                .map_err(|_| AppError::Conflict("模型发布响应无效，请核对物理结构".into()))
        };
        let result = tokio::time::timeout(Duration::from_secs(60), operation)
            .await
            .map_err(|_| AppError::Conflict("模型发布超时，物理结果待核对".into()))??;
        let verify = async {
            let binding = self
                .binding(properties)
                .await?
                .ok_or_else(|| AppError::Conflict("模型发布后未找到实际基础表".into()))?;
            self.verified_receipt(properties, ddl, binding, false, Some(result.record_count))
                .await
        }
        .await;
        verify.map_err(|_| {
            AppError::Conflict("建表请求已有引擎响应，但物理结构未完整核对；结果待核对".into())
        })
    }

    async fn binding(&self, properties: &Value) -> Result<Option<Value>, AppError> {
        generate_ddl(properties)?;
        let name = properties["table_name"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("物理表名缺失".into()))?;
        let (schema, table) = name.split_once('.').unwrap_or(("main", name));
        let sql = format!(
            "SELECT table_catalog, table_schema, table_name, table_type FROM information_schema.tables WHERE table_catalog=current_database() AND table_schema='{}' AND table_name='{}'",
            sql_text(schema),
            sql_text(table)
        );
        physical_binding(schema, table, &self.query.execute(&sql).await?.rows)
    }

    async fn columns(&self, binding: &Value) -> Result<Vec<Value>, AppError> {
        let filter = binding_filter(binding, "")?;
        let sql = format!(
            "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE {filter} ORDER BY ordinal_position"
        );
        Ok(self.query.execute(&sql).await?.rows)
    }

    async fn verify_constraints(
        &self,
        properties: &Value,
        binding: &Value,
    ) -> Result<(), AppError> {
        let filter = binding_filter(binding, "c.")?;
        let sql = format!(
            "SELECT k.column_name FROM information_schema.table_constraints AS c JOIN information_schema.key_column_usage AS k ON c.constraint_catalog=k.constraint_catalog AND c.constraint_schema=k.constraint_schema AND c.constraint_name=k.constraint_name AND c.table_catalog=k.table_catalog AND c.table_schema=k.table_schema AND c.table_name=k.table_name WHERE {filter} AND c.constraint_type='PRIMARY KEY' ORDER BY k.ordinal_position"
        );
        let actual = self.query.execute(&sql).await?.rows;
        let expected = properties["primary_key"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if actual.len() != expected.len()
            || actual.iter().zip(&expected).any(|(row, name)| {
                !row["column_name"].as_str().is_some_and(|actual| {
                    name.as_str()
                        .is_some_and(|expected| actual.eq_ignore_ascii_case(expected))
                })
            })
        {
            return Err(AppError::Conflict("目标物理主键与模型定义不一致".into()));
        }
        Ok(())
    }
}

fn sql_text(text: &str) -> String {
    text.replace('\'', "''")
}

fn physical_binding(schema: &str, table: &str, rows: &[Value]) -> Result<Option<Value>, AppError> {
    if rows.is_empty() {
        return Ok(None);
    }
    if rows.len() != 1 {
        return Err(AppError::Conflict("目标物理对象身份不唯一".into()));
    }
    let row = &rows[0];
    let catalog = row["table_catalog"]
        .as_str()
        .filter(|value| {
            !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| AppError::Conflict("实际物理库身份无效".into()))?;
    if row["table_schema"].as_str() != Some(schema)
        || row["table_name"].as_str() != Some(table)
        || row["table_type"].as_str() != Some("BASE TABLE")
    {
        return Err(AppError::Conflict("目标物理对象不是匹配的基础表".into()));
    }
    Ok(Some(
        json!({"table_catalog":catalog,"schema_name":schema,"table_name":table,"table_type":"BASE TABLE"}),
    ))
}

fn binding_filter(binding: &Value, prefix: &str) -> Result<String, AppError> {
    let fields = [
        ("table_catalog", "table_catalog"),
        ("schema_name", "table_schema"),
        ("table_name", "table_name"),
    ];
    fields
        .into_iter()
        .map(|(key, column)| {
            let value = binding[key]
                .as_str()
                .ok_or_else(|| AppError::Conflict("物理对象回执不完整".into()))?;
            Ok(format!("{prefix}{column}='{}'", sql_text(value)))
        })
        .collect::<Result<Vec<_>, AppError>>()
        .map(|filters| filters.join(" AND "))
}

fn verify_columns(properties: &Value, physical: &[Value]) -> Result<(), AppError> {
    let columns = properties["columns"]
        .as_array()
        .ok_or_else(|| AppError::BadRequest("模型字段缺失".into()))?;
    if columns.len() != physical.len() {
        return Err(AppError::Conflict(
            "实际物理表字段数量与模型不一致，不会覆盖已有表".into(),
        ));
    }
    let primary_keys = properties["primary_key"].as_array();
    for (expected, actual) in columns.iter().zip(physical) {
        let expected_type = crate::services::product_domain::warehouse::canonical_data_type(
            expected["data_type"].as_str().unwrap_or_default(),
        )?;
        let actual_type = crate::services::product_domain::warehouse::canonical_data_type(
            actual["data_type"].as_str().unwrap_or_default(),
        )?;
        let nullable = expected["nullable"].as_bool().unwrap_or(true)
            && !primary_keys.is_some_and(|keys| {
                keys.iter().any(|key| {
                    key.as_str().is_some_and(|key| {
                        expected["name"]
                            .as_str()
                            .is_some_and(|name| key.eq_ignore_ascii_case(name))
                    })
                })
            });
        if expected["name"] != actual["column_name"]
            || expected_type != actual_type
            || nullable != (actual["is_nullable"].as_str() == Some("YES"))
        {
            return Err(AppError::Conflict(
                "实际物理表结构与模型不一致，请先核对差异".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_table_is_not_accepted_when_type_or_nullability_differs() {
        let model = json!({"columns":[{"name":"id","data_type":"BIGINT","nullable":true}],"primary_key":["id"]});
        assert!(
            verify_columns(
                &model,
                &[json!({"column_name":"id","data_type":"BIGINT","is_nullable":"NO"})]
            )
            .is_ok()
        );
        assert!(
            verify_columns(
                &model,
                &[json!({"column_name":"id","data_type":"VARCHAR","is_nullable":"NO"})]
            )
            .is_err()
        );
        assert!(
            verify_columns(
                &model,
                &[json!({"column_name":"id","data_type":"BIGINT","is_nullable":"YES"})]
            )
            .is_err()
        );
    }
    #[test]
    fn actual_catalog_is_preserved_and_all_followup_queries_use_exact_identity() {
        let row = json!({"table_catalog":"warehouse'1","table_schema":"main","table_name":"orders","table_type":"BASE TABLE"});
        let binding = physical_binding("main", "orders", &[row.clone()])
            .unwrap()
            .unwrap();
        assert_eq!(binding["table_catalog"], "warehouse'1");
        assert_eq!(
            binding_filter(&binding, "c.").unwrap(),
            "c.table_catalog='warehouse''1' AND c.table_schema='main' AND c.table_name='orders'"
        );
        assert!(physical_binding("main", "orders", &[row.clone(), row]).is_err());
        assert!(physical_binding("main", "orders", &[json!({"table_catalog":"warehouse","table_schema":"main","table_name":"orders","table_type":"VIEW"})]).is_err());
        assert!(physical_binding("main", "orders", &[json!({"table_catalog":"warehouse","table_schema":"other","table_name":"orders","table_type":"BASE TABLE"})]).is_err());
    }
}
