use super::bad;
use crate::{models::error::AppError, services::query_svc::ColumnMeta};
use serde_json::Value;

fn decimal_parts(value: &str) -> Option<(&str, &str)> {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let mut parts = unsigned.split('.');
    let integer = parts.next()?;
    let fraction = parts.next().unwrap_or("");
    if integer.is_empty()
        || !integer.bytes().all(|c| c.is_ascii_digit())
        || !fraction.bytes().all(|c| c.is_ascii_digit())
        || parts.next().is_some()
        || (integer.len() > 1 && integer.starts_with('0'))
        || value.ends_with('.')
        || value.starts_with('+')
    {
        return None;
    }
    Some((integer, fraction))
}
pub(super) fn factor(value: f64) -> Result<String, AppError> {
    let text = value.to_string();
    let (integer, fraction) =
        decimal_parts(&text).ok_or_else(|| bad("单位系数必须可表示为有限十进制"))?;
    if !value.is_finite()
        || value <= 0.0
        || integer.trim_start_matches('0').len() + fraction.len() > 38
        || fraction.len() > 18
    {
        return Err(bad("单位换算系数超出精确DECIMAL(38,18)口径"));
    }
    Ok(text)
}
pub(super) fn reciprocal(value: f64) -> Result<String, AppError> {
    let text = factor(value)?;
    let (integer, fraction) = decimal_parts(&text).ok_or_else(|| bad("单位系数无效"))?;
    let mut denominator: u128 = format!("{integer}{fraction}")
        .parse()
        .map_err(|_| bad("单位系数过大"))?;
    let mut numerator = 10u128.pow(fraction.len() as u32);
    let (mut a, mut b) = (numerator, denominator);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    numerator /= a;
    denominator /= a;
    let mut rest = denominator;
    let (mut twos, mut fives) = (0, 0);
    while rest % 2 == 0 {
        rest /= 2;
        twos += 1;
    }
    while rest % 5 == 0 {
        rest /= 5;
        fives += 1;
    }
    let scale = twos.max(fives);
    if rest != 1 || scale > 18 {
        return Err(bad("输出单位倒数不是有限18位十进制，禁止默默近似换算"));
    }
    let encoded = numerator
        .checked_mul(10u128.pow(scale))
        .ok_or_else(|| bad("输出单位换算溢出"))?
        / denominator;
    let mut encoded = encoded.to_string();
    if scale > 0 {
        while encoded.len() <= scale as usize {
            encoded.insert(0, '0');
        }
        encoded.insert(encoded.len() - scale as usize, '.');
        while encoded.ends_with('0') {
            encoded.pop();
        }
        if encoded.ends_with('.') {
            encoded.pop();
        }
    }
    if encoded.replace('.', "").trim_start_matches('0').len() > 38 {
        return Err(bad("输出单位换算超出DECIMAL精度"));
    }
    Ok(encoded)
}
pub(super) fn unsigned_count(value: &Value) -> Option<u64> {
    if let Some(number) = value.as_u64() {
        return Some(number);
    }
    let text = value.as_str()?;
    let (integer, fraction) = decimal_parts(text)?;
    if text.starts_with('-') || !fraction.is_empty() {
        return None;
    }
    integer.parse().ok()
}
pub(super) fn numeric_value(value: &Value, column: &ColumnMeta) -> bool {
    if !column.is_numeric {
        return false;
    }
    let ty = &column.data_type;
    if let Some((prefix, arguments)) = ty.split_once('(') {
        if matches!(
            prefix,
            "Decimal32" | "Decimal64" | "Decimal128" | "Decimal256" | "DECIMAL"
        ) {
            let Some((precision, scale)) = arguments
                .strip_suffix(')')
                .and_then(|args| args.split_once(','))
            else {
                return false;
            };
            let Some((precision, scale)) = precision
                .trim()
                .parse::<usize>()
                .ok()
                .zip(scale.trim().parse::<usize>().ok())
            else {
                return false;
            };
            let Some((integer, fraction)) = value.as_str().and_then(decimal_parts) else {
                return false;
            };
            return precision > 0
                && precision <= 76
                && scale <= precision
                && integer.trim_start_matches('0').len() <= precision - scale
                && fraction.len() == scale;
        }
    }
    match ty.as_str() {
        "Int8" | "Int16" | "Int32" | "Int64" | "BIGINT" | "INTEGER" => {
            let number = value.as_i64().or_else(|| {
                let text = value.as_str()?;
                decimal_parts(text).filter(|(_, fraction)| fraction.is_empty())?;
                text.parse::<i64>()
                    .ok()
                    .filter(|number| number.to_string() == text)
            });
            number.is_some_and(|number| match ty.as_str() {
                "Int8" => i8::try_from(number).is_ok(),
                "Int16" => i16::try_from(number).is_ok(),
                "Int32" | "INTEGER" => i32::try_from(number).is_ok(),
                _ => true,
            })
        },
        "UInt8" | "UInt16" | "UInt32" | "UInt64" | "UBIGINT" => {
            unsigned_count(value).is_some_and(|number| match ty.as_str() {
                "UInt8" => u8::try_from(number).is_ok(),
                "UInt16" => u16::try_from(number).is_ok(),
                "UInt32" => u32::try_from(number).is_ok(),
                _ => true,
            })
        },
        "Float32" | "Float64" | "FLOAT" | "DOUBLE" => value.as_f64().is_some_and(f64::is_finite),
        _ => false,
    }
}
