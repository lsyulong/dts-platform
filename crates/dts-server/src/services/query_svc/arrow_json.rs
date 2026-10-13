// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use arrow_array::*;
use arrow_cast::{cast, display::array_value_to_string};
use arrow_schema::{DataType, SchemaRef};
use serde_json::{Map, Value};

use super::{ColumnMeta, QueryLimits};
use crate::models::error::AppError;

/// Collects the preview while applying one budget across all Flight batches.
pub(super) struct ResultCollector {
    schema: SchemaRef,
    columns: Vec<ColumnMeta>,
    rows: Vec<Value>,
    remaining_bytes: usize,
    limits: QueryLimits,
}

impl ResultCollector {
    pub(super) fn new(schema: SchemaRef, limits: QueryLimits) -> Result<Self, AppError> {
        unique_fields(&schema)?;
        if schema.fields().len() > 512 {
            return Err(AppError::BadRequest(
                "结果超过 512 列，请选择需要的列".into(),
            ));
        }
        let columns = schema
            .fields()
            .iter()
            .map(|field| ColumnMeta {
                name: field.name().clone(),
                data_type: format!("{:?}", field.data_type()),
                is_numeric: field.data_type().is_numeric(),
            })
            .collect::<Vec<_>>();
        let metadata_size = serde_json::to_vec(&columns)
            .map_err(|_| AppError::Internal("结果结构序列化失败".into()))?
            .len();
        let remaining_bytes = limits
            .max_json_bytes
            .checked_sub(metadata_size + 2)
            .ok_or_else(byte_limit_error)?;
        Ok(Self {
            schema,
            columns,
            rows: Vec::new(),
            remaining_bytes,
            limits,
        })
    }

    pub(super) fn append(&mut self, batch: &RecordBatch) -> Result<(), AppError> {
        if batch.schema().fields() != self.schema.fields() {
            return Err(AppError::Internal(
                "分析引擎在同一次查询中返回了不同的结果结构".into(),
            ));
        }
        if batch.get_array_memory_size() > self.limits.max_batch_bytes {
            return Err(AppError::BadRequest(
                "结果批次过大，请减少查询列或结果范围".into(),
            ));
        }
        if batch.num_rows() > self.limits.max_rows.saturating_sub(self.rows.len()) {
            return Err(AppError::BadRequest(format!(
                "结果超过 {} 行，请添加 LIMIT 或缩小查询范围",
                self.limits.max_rows
            )));
        }
        let mut budget = JsonBudget {
            remaining: self.remaining_bytes,
        };
        for row in 0..batch.num_rows() {
            if !self.rows.is_empty() {
                budget.take(1)?;
            }
            self.rows.push(row_to_json(batch, row, &mut budget)?);
        }
        self.remaining_bytes = budget.remaining;
        Ok(())
    }

    pub(super) fn into_parts(self) -> (Vec<ColumnMeta>, Vec<Value>) {
        (self.columns, self.rows)
    }
}

fn unique_fields(schema: &arrow_schema::Schema) -> Result<(), AppError> {
    let mut names = HashSet::new();
    if schema
        .fields()
        .iter()
        .any(|field| !names.insert(field.name()))
    {
        return Err(AppError::BadRequest(
            "结果列名重复，请使用不同的 AS 别名".into(),
        ));
    }
    Ok(())
}

fn byte_limit_error() -> AppError {
    AppError::BadRequest("结果 JSON 超过大小限制，请减少查询列或缩小查询范围".into())
}

struct JsonBudget {
    remaining: usize,
}

impl JsonBudget {
    fn take(&mut self, bytes: usize) -> Result<(), AppError> {
        self.remaining = self
            .remaining
            .checked_sub(bytes)
            .ok_or_else(byte_limit_error)?;
        Ok(())
    }

    fn charge_string(&mut self, value: &str) -> Result<(), AppError> {
        // Count the escaped JSON bytes before allocating a copy of the value.
        let size = value
            .bytes()
            .try_fold(2usize, |size, byte| {
                size.checked_add(match byte {
                    b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
                    0..=31 => 6,
                    _ => 1,
                })
            })
            .ok_or_else(byte_limit_error)?;
        self.take(size)
    }

    fn string(&mut self, value: &str) -> Result<Value, AppError> {
        self.charge_string(value)?;
        Ok(Value::String(value.to_owned()))
    }

    fn scalar(&mut self, value: Value) -> Result<Value, AppError> {
        self.take(
            serde_json::to_vec(&value)
                .map_err(|_| AppError::Internal("结果值序列化失败".into()))?
                .len(),
        )?;
        Ok(value)
    }
}

fn row_to_json(
    batch: &RecordBatch,
    row: usize,
    budget: &mut JsonBudget,
) -> Result<Value, AppError> {
    budget.take(2)?;
    let schema = batch.schema();
    let mut fields = Map::with_capacity(batch.num_columns());
    for (index, (field, array)) in schema.fields().iter().zip(batch.columns()).enumerate() {
        if index > 0 {
            budget.take(1)?;
        }
        budget.charge_string(field.name())?;
        budget.take(1)?;
        fields.insert(
            field.name().clone(),
            value_to_json(array.as_ref(), row, budget, 0)?,
        );
    }
    Ok(Value::Object(fields))
}

fn values_to_json(
    array: &dyn Array,
    budget: &mut JsonBudget,
    depth: usize,
) -> Result<Value, AppError> {
    budget.take(2)?;
    let mut values = Vec::new();
    for index in 0..array.len() {
        if index > 0 {
            budget.take(1)?;
        }
        values.push(value_to_json(array, index, budget, depth + 1)?);
    }
    Ok(Value::Array(values))
}

/// Decimal and integers outside the JavaScript exact range use strings; binary
/// uses hex. Unsupported types fail explicitly instead of changing data to NULL.
fn value_to_json(
    array: &dyn Array,
    index: usize,
    budget: &mut JsonBudget,
    depth: usize,
) -> Result<Value, AppError> {
    if depth > 16 {
        return Err(AppError::BadRequest(
            "结果嵌套超过 16 层，请展开需要的字段".into(),
        ));
    }
    if array.is_null(index) {
        return budget.scalar(Value::Null);
    }
    macro_rules! number {
        ($ty:ty) => {
            if let Some(values) = array.as_any().downcast_ref::<$ty>() {
                return budget.scalar(serde_json::json!(values.value(index)));
            }
        };
    }
    number!(Int8Array);
    number!(Int16Array);
    number!(Int32Array);
    number!(UInt8Array);
    number!(UInt16Array);
    number!(UInt32Array);
    if let Some(values) = array.as_any().downcast_ref::<Int64Array>() {
        let value = values.value(index);
        return if value.unsigned_abs() <= 9_007_199_254_740_991 {
            budget.scalar(serde_json::json!(value))
        } else {
            budget.string(&value.to_string())
        };
    }
    if let Some(values) = array.as_any().downcast_ref::<UInt64Array>() {
        let value = values.value(index);
        return if value <= 9_007_199_254_740_991 {
            budget.scalar(serde_json::json!(value))
        } else {
            budget.string(&value.to_string())
        };
    }
    macro_rules! float {
        ($ty:ty) => {
            if let Some(values) = array.as_any().downcast_ref::<$ty>() {
                let value = values.value(index);
                return if value.is_finite() {
                    budget.scalar(serde_json::json!(value))
                } else {
                    budget.string(&value.to_string())
                };
            }
        };
    }
    float!(Float32Array);
    float!(Float64Array);
    if let Some(values) = array.as_any().downcast_ref::<BooleanArray>() {
        return budget.scalar(Value::Bool(values.value(index)));
    }
    macro_rules! text {
        ($ty:ty) => {
            if let Some(values) = array.as_any().downcast_ref::<$ty>() {
                return budget.string(values.value(index));
            }
        };
    }
    text!(StringArray);
    text!(LargeStringArray);
    text!(StringViewArray);

    macro_rules! list {
        ($ty:ty) => {
            if let Some(values) = array.as_any().downcast_ref::<$ty>() {
                return values_to_json(values.value(index).as_ref(), budget, depth);
            }
        };
    }
    list!(ListArray);
    list!(LargeListArray);
    list!(FixedSizeListArray);
    list!(ListViewArray);
    list!(LargeListViewArray);
    if let Some(values) = array.as_any().downcast_ref::<MapArray>() {
        return values_to_json(&values.value(index), budget, depth);
    }
    if let Some(values) = array.as_any().downcast_ref::<StructArray>() {
        unique_fields(&arrow_schema::Schema::new(values.fields().clone()))?;
        budget.take(2)?;
        let mut object = Map::new();
        for (field_index, (field, column)) in
            values.fields().iter().zip(values.columns()).enumerate()
        {
            if field_index > 0 {
                budget.take(1)?;
            }
            budget.charge_string(field.name())?;
            budget.take(1)?;
            object.insert(
                field.name().clone(),
                value_to_json(column.as_ref(), index, budget, depth + 1)?,
            );
        }
        return Ok(Value::Object(object));
    }
    if let Some(values) = array.as_any().downcast_ref::<UnionArray>() {
        return value_to_json(
            values.child(values.type_id(index)).as_ref(),
            values.value_offset(index),
            budget,
            depth + 1,
        );
    }
    if let DataType::Dictionary(_, value_type) = array.data_type() {
        let decoded = cast(array.slice(index, 1).as_ref(), value_type.as_ref())
            .map_err(|_| AppError::Internal("结果字典类型转换失败".into()))?;
        return value_to_json(decoded.as_ref(), 0, budget, depth + 1);
    }
    macro_rules! run {
        ($ty:ty) => {
            if let Some(values) = array.as_any().downcast_ref::<RunArray<$ty>>() {
                return value_to_json(
                    values.values().as_ref(),
                    values.get_physical_index(index),
                    budget,
                    depth + 1,
                );
            }
        };
    }
    run!(types::Int16Type);
    run!(types::Int32Type);
    run!(types::Int64Type);

    match array.data_type() {
        DataType::Null => budget.scalar(Value::Null),
        DataType::Decimal32(_, _)
        | DataType::Decimal64(_, _)
        | DataType::Decimal128(_, _)
        | DataType::Decimal256(_, _)
        | DataType::Date32
        | DataType::Date64
        | DataType::Time32(_)
        | DataType::Time64(_)
        | DataType::Timestamp(_, _)
        | DataType::Duration(_)
        | DataType::Interval(_)
        | DataType::Binary
        | DataType::LargeBinary
        | DataType::BinaryView
        | DataType::FixedSizeBinary(_)
        | DataType::Float16 => {
            let value = array_value_to_string(array, index)
                .map_err(|_| AppError::Internal("结果值格式转换失败".into()))?;
            budget.string(&value)
        },
        data_type => Err(AppError::Internal(format!(
            "暂不支持结果类型 {data_type:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow_array::builder::{Int32Builder, ListBuilder, StringDictionaryBuilder};
    use arrow_schema::{Field, Schema};

    use super::*;

    fn collect(batch: RecordBatch, limits: QueryLimits) -> anyhow::Result<Vec<Value>> {
        let mut collector = ResultCollector::new(batch.schema(), limits)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        collector
            .append(&batch)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(collector.into_parts().1)
    }

    #[test]
    fn arrow_json_preserves_decimal_date_binary_and_nested_values() -> anyhow::Result<()> {
        let decimal = Decimal128Array::from(vec![Some(12345678901234567890_i128), None])
            .with_precision_and_scale(25, 2)?;
        let nested = StructArray::from(vec![(
            Arc::new(Field::new("id", DataType::Int64, false)),
            Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef,
        )]);
        let arrays: Vec<ArrayRef> = vec![
            Arc::new(decimal),
            Arc::new(Date32Array::from(vec![Some(0), Some(1)])),
            Arc::new(BinaryArray::from(vec![Some(b"\x00\xff".as_slice()), None])),
            Arc::new(nested),
        ];
        let schema = Arc::new(Schema::new(
            ["amount", "day", "payload", "item"]
                .iter()
                .zip(&arrays)
                .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
                .collect::<Vec<_>>(),
        ));
        let rows = collect(
            RecordBatch::try_new(schema, arrays)?,
            QueryLimits::default(),
        )?;
        assert_eq!(rows[0]["amount"], "123456789012345678.90");
        assert_eq!(rows[0]["day"], "1970-01-01");
        assert_eq!(rows[0]["payload"], "00ff");
        assert_eq!(rows[0]["item"], serde_json::json!({"id": 1}));
        assert_eq!(rows[1]["amount"], Value::Null);
        Ok(())
    }

    #[test]
    fn preserves_large_integers_lists_dictionary_and_nonfinite_numbers() -> anyhow::Result<()> {
        let mut dictionary = StringDictionaryBuilder::<types::Int8Type>::new();
        dictionary.append("a")?;
        dictionary.append("b")?;
        let mut lists = ListBuilder::new(Int32Builder::new());
        lists.values().append_value(1);
        lists.values().append_null();
        lists.append(true);
        lists.append(true);
        let arrays: Vec<ArrayRef> = vec![
            Arc::new(Int64Array::from(vec![i64::MIN, 42])),
            Arc::new(lists.finish()),
            Arc::new(dictionary.finish()),
            Arc::new(Float64Array::from(vec![f64::NAN, f64::INFINITY])),
        ];
        let schema = Arc::new(Schema::new(
            ["id", "items", "label", "float"]
                .iter()
                .zip(&arrays)
                .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
                .collect::<Vec<_>>(),
        ));
        let rows = collect(
            RecordBatch::try_new(schema, arrays)?,
            QueryLimits::default(),
        )?;
        assert_eq!(rows[0]["id"], i64::MIN.to_string());
        assert_eq!(rows[1]["id"], 42);
        assert_eq!(rows[0]["items"], serde_json::json!([1, null]));
        assert_eq!(rows[1]["items"], serde_json::json!([]));
        assert_eq!(rows[0]["label"], "a");
        assert_eq!(rows[0]["float"], "NaN");
        assert_eq!(rows[1]["float"], "inf");
        Ok(())
    }

    #[test]
    fn rejects_duplicate_column_names() -> anyhow::Result<()> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("id", DataType::Int32, false),
        ]));
        assert!(ResultCollector::new(schema, QueryLimits::default()).is_err());
        Ok(())
    }

    #[test]
    fn counts_escaped_json_before_allocating_strings() {
        let mut budget = JsonBudget { remaining: 7 };
        assert!(budget.string("\u{0000}\n").is_err());
        let mut budget = JsonBudget { remaining: 10 };
        assert!(budget.string("\u{0000}\n").is_ok());
        assert_eq!(budget.remaining, 0);
    }
}
