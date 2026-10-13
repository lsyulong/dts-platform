use super::{bad, types::*};
use crate::models::error::AppError;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

pub(super) fn validate_period(period: &PeriodDefinition) -> Result<(), AppError> {
    if !matches!(period.unit.as_str(), "day" | "week" | "month" | "year")
        || !(1..=10000).contains(&period.length)
    {
        return Err(bad("周期单位或1至10000的整数长度无效"));
    }
    period
        .timezone
        .parse::<Tz>()
        .map_err(|_| bad("周期时区必须是有效IANA时区"))?;
    Ok(())
}
fn midnight(tz: Tz, date: NaiveDate) -> Result<DateTime<Utc>, AppError> {
    tz.from_local_datetime(
        &date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| bad("周期午夜无效"))?,
    )
    .single()
    .map(|date| date.with_timezone(&Utc))
    .ok_or_else(|| bad("周期边界处于不存在或歧义的本地时间，必须明确核对"))
}
pub fn completed_window(
    period: &PeriodDefinition,
    as_of: DateTime<Utc>,
) -> Result<MetricWindow, AppError> {
    validate_period(period)?;
    let tz: Tz = period.timezone.parse().map_err(|_| bad("周期时区无效"))?;
    let date = as_of.with_timezone(&tz).date_naive();
    let end = match period.unit.as_str() {
        "day" => date,
        "week" => date
            .checked_sub_signed(Duration::days(date.weekday().num_days_from_monday() as i64))
            .ok_or_else(|| bad("周边界溢出"))?,
        "month" => NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
            .ok_or_else(|| bad("月边界溢出"))?,
        "year" => NaiveDate::from_ymd_opt(date.year(), 1, 1).ok_or_else(|| bad("年边界溢出"))?,
        _ => return Err(bad("周期单位无效")),
    };
    let start = if matches!(period.unit.as_str(), "month" | "year") {
        let months = i64::from(end.year()) * 12 + i64::from(end.month0())
            - i64::from(period.length) * if period.unit == "year" { 12 } else { 1 };
        let year: i32 = months
            .div_euclid(12)
            .try_into()
            .map_err(|_| bad("周期年份溢出"))?;
        NaiveDate::from_ymd_opt(year, (months.rem_euclid(12) + 1) as u32, 1)
            .ok_or_else(|| bad("周期起点溢出"))?
    } else {
        end.checked_sub_signed(Duration::days(
            i64::from(period.length) * if period.unit == "week" { 7 } else { 1 },
        ))
        .ok_or_else(|| bad("周期长度溢出"))?
    };
    let window = MetricWindow {
        start: midnight(tz, start)?,
        end: midnight(tz, end)?,
        timezone: period.timezone.clone(),
        source: "completed_calendar_periods".into(),
    };
    validate_window(&window)?;
    Ok(window)
}
pub(super) fn validate_window(window: &MetricWindow) -> Result<(), AppError> {
    if window.start >= window.end || window.end - window.start > Duration::days(3660) {
        return Err(bad("数据窗口必须递增且不超过3660天"));
    }
    Ok(())
}
pub(super) fn resolve(
    plan: &MetricPlan,
    request: &MetricRunRequest,
) -> Result<MetricWindow, AppError> {
    if !(1..=1000).contains(&request.row_limit) {
        return Err(bad("指标结果上限须为1至1000"));
    }
    let periods = plan
        .measures
        .iter()
        .filter_map(|measure| measure.period.as_ref())
        .collect::<Vec<_>>();
    if periods.is_empty() {
        if request.as_of.is_some() {
            return Err(bad("原子计算使用明确起止数据窗口，不接受被忽略的as_of"));
        }
        let window = MetricWindow {
            start: request
                .window_start
                .ok_or_else(|| bad("计算必须指定window_start"))?,
            end: request
                .window_end
                .ok_or_else(|| bad("计算必须指定window_end"))?,
            timezone: plan.time_timezone.clone(),
            source: "explicit_half_open_window".into(),
        };
        validate_window(&window)?;
        Ok(window)
    } else {
        if request.window_start.is_some() || request.window_end.is_some() {
            return Err(bad("派生周期计算只接受as_of，窗口由已发布周期生成"));
        }
        let as_of = request
            .as_of
            .ok_or_else(|| bad("周期计算必须指定稳定的as_of以支持幂等重试"))?;
        let first = completed_window(periods[0], as_of)?;
        for period in &periods[1..] {
            if completed_window(period, as_of)? != first {
                return Err(bad("复合指标依赖的周期、时区或窗口不对齐"));
            }
        }
        Ok(first)
    }
}
