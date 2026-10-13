// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, bail};

/// Bounded POSIX fields accepted by the platform's JiaScheduler adapter.
/// Extended Quartz operators are deliberately outside this adapter contract.
pub(crate) fn fields(expression: &str) -> Result<Vec<String>> {
    if expression.len() > 256 || expression.chars().any(char::is_control) {
        bail!("Cron 表达式过长或包含控制字符");
    }
    let mut fields: Vec<_> = expression.split_whitespace().collect();
    if fields.len() == 5 {
        fields.insert(0, "0");
    }
    if fields.len() != 6 {
        bail!("Cron 必须是五字段，或包含秒的六字段；不支持年份字段");
    }
    let bounds = [(0, 59), (0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];
    fields
        .iter()
        .enumerate()
        .map(|(index, field)| normalize(field, index, bounds[index]))
        .collect()
}

fn ordinal(value: &str, index: usize, (minimum, maximum): (u32, u32)) -> Result<u32> {
    let upper = value.to_ascii_uppercase();
    let named = match index {
        4 => [
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
        ]
        .iter()
        .position(|name| *name == upper)
        .map(|v| v as u32 + 1),
        5 => ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]
            .iter()
            .position(|name| *name == upper)
            .map(|v| v as u32),
        _ => None,
    };
    let number = if let Some(number) = named {
        number
    } else if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        value
            .parse::<u32>()
            .map_err(|_| anyhow::anyhow!("Cron 数值无效"))?
    } else {
        bail!("Cron 字段包含不支持的值或操作符");
    };
    if !(minimum..=maximum).contains(&number) {
        bail!(
            "Cron 第 {} 个字段超出 {} 至 {} 的范围",
            index + 1,
            minimum,
            maximum
        );
    }
    Ok(number)
}

fn normalize(field: &str, index: usize, bounds: (u32, u32)) -> Result<String> {
    if field == "?" && matches!(index, 3 | 5) {
        return Ok("*".into());
    }
    field
        .split(',')
        .map(|part| {
            let (base, step) = match part.split_once('/') {
                Some((base, step)) => {
                    if step.is_empty() || !step.bytes().all(|b| b.is_ascii_digit()) {
                        bail!("Cron 步长必须为正整数");
                    }
                    let number = step
                        .parse::<u32>()
                        .map_err(|_| anyhow::anyhow!("Cron 步长无效"))?;
                    if number == 0 || number > bounds.1 - bounds.0 + 1 {
                        bail!("Cron 步长为零或超过当前字段范围");
                    }
                    (base, Some(number))
                },
                None => (part, None),
            };
            let normalized = if base == "*" {
                "*".into()
            } else if let Some((start, end)) = base.split_once('-') {
                let start = ordinal(start, index, bounds)?;
                let end = ordinal(end, index, bounds)?;
                if start > end {
                    bail!("Cron 范围起点不能大于终点");
                }
                format!("{start}-{end}")
            } else {
                ordinal(base, index, bounds)?.to_string()
            };
            Ok(match step {
                Some(step) => format!("{normalized}/{step}"),
                None => normalized,
            })
        })
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_ranges_steps_and_unsupported_fields() {
        for expression in [
            "60 * * * * *",
            "0 60 * * * *",
            "0 0 24 * * *",
            "0 0 0 0 * *",
            "0 0 0 * 13 *",
            "0 0 0 * * 8",
            "*/0 * * * *",
            "*/999 * * * *",
            "5-1 * * * *",
            "1,,2 * * * *",
            "1/2/3 * * * *",
            "x * * * *",
            "? * * * *",
            "0 0 0 * * MON#2",
            "0 0 0 L * *",
            "0 0 0 * * * 2026",
            "0 0 0 * * *\n",
            "0 0 0 * * FUNDAY",
        ] {
            assert!(fields(expression).is_err(), "accepted {expression}");
        }
    }

    #[test]
    fn normalizes_posix_names_and_preserves_sunday_and_seconds() {
        assert_eq!(
            fields("*/5 09 * jan,DEC MON-FRI").unwrap().join(" "),
            "0 */5 9 * 1,12 1-5"
        );
        assert_eq!(
            fields("1-59/2 0 23 ? * 0,7").unwrap().join(" "),
            "1-59/2 0 23 * * 0,7"
        );
        assert_eq!(fields("0 0 0 * * sun").unwrap().join(" "), "0 0 0 * * 0");
        assert_eq!(fields("0/15 * * * *").unwrap().join(" "), "0 0/15 * * * *");
    }

    #[test]
    fn typed_timer_cannot_smuggle_an_empty_or_extra_field() {
        let mut timer = crate::models::TimerExpr::from_cron("0 8 * * *").unwrap();
        assert!(timer.validate().is_ok());
        timer.second.clear();
        assert!(timer.validate().is_err());
        timer.second = "0 1".into();
        assert!(timer.validate().is_err());
    }
}
